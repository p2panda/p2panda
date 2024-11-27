// SPDX-License-Identifier: AGPL-3.0-or-later

mod config;
mod handler;
pub(crate) mod manager;

pub use config::{ResyncConfiguration, SyncConfiguration};
pub use handler::{SyncConnection, SYNC_CONNECTION_ALPN};
use tokio::task::JoinHandle;

use std::sync::Arc;

use anyhow::Result;
use futures_util::{AsyncRead, AsyncWrite, SinkExt};
use iroh_net::key::PublicKey;
use p2panda_sync::{FromSync, SyncError, SyncProtocol, Topic};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::PollSender;
use tracing::{debug, error};

use crate::engine::ToEngineActor;
use crate::TopicId;

/// Initiate a sync protocol session over the provided bi-directional stream for the given peer and
/// topic.
pub async fn initiate_sync<T, S, R>(
    mut send: &mut S,
    mut recv: &mut R,
    peer: PublicKey,
    topic: T,
    sync_protocol: Arc<dyn for<'a> SyncProtocol<'a, T> + 'static>,
    engine_actor_tx: mpsc::Sender<ToEngineActor<T>>,
) -> Result<(), SyncError>
where
    T: Topic + TopicId + 'static,
    S: AsyncWrite + Send + Unpin,
    R: AsyncRead + Send + Unpin,
{
    debug!(
        "initiate sync session with peer {} over topic {:?}",
        peer, topic
    );

    engine_actor_tx
        .send(ToEngineActor::SyncStart {
            topic: topic.clone(),
            peer,
        })
        .await
        .map_err(|err| SyncError::Critical(format!("engine_actor_tx failed: {err}")))?;

    // Set up a channel for receiving messages from the sync session.
    let (tx, mut rx) = mpsc::channel::<FromSync<T>>(128);
    let mut sink = PollSender::new(tx).sink_map_err(|e| SyncError::Critical(e.to_string()));

    // Spawn a "glue" task which represents the layer between the sync session and the engine.
    //
    // It picks up any messages from the sync session makes sure that the "Two-Phase Sync Flow" is
    // followed (I. "Handshake" Phase & II. "Data Sync" Phase) and the engine accordingly informed
    // about it.
    //
    // If the task detects any invalid behaviour of the sync flow, it fails critically, indicating
    // that the sync protocol implementation does not behave correctly and is not compatible with
    // the engine.
    //
    // Additionally, the task forwards any synced application data straight to the engine.
    let glue_task_handle: JoinHandle<Result<(), SyncError>> = {
        let mut sync_handshake_success = false;
        let topic = topic.clone();

        tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                // I. Handshake Phase.
                //
                // At the beginning of every sync session the "initiating" peer needs to send over
                // the topic to the "accepting" peer during the handshake phase. This is the first
                // message we're expecting:
                if let FromSync::HandshakeSuccess(_) = message {
                    // Receiving the handshake message twice is a protocol violation.
                    if sync_handshake_success {
                        return Err(SyncError::Critical(
                            "received handshake message twice from sync session in handshake phase"
                                .into(),
                        ));
                    }
                    sync_handshake_success = true;

                    // Inform the engine that we are expecting sync messages from the peer on this
                    // topic.
                    engine_actor_tx
                        .send(ToEngineActor::SyncHandshakeSuccess {
                            peer,
                            topic: topic.clone(),
                        })
                        .await
                        .map_err(|err| {
                            SyncError::Critical(format!("engine_actor_tx failed: {err}"))
                        })?;

                    continue;
                }

                // 2. Data Sync Phase.
                // ~~~~~~~~~~~~~~~~~~~
                let FromSync::Data(header, payload) = message else {
                    return Err(SyncError::Critical("expected to receive only data messages from sync session in data sync phase".into()));
                };

                engine_actor_tx
                    .send(ToEngineActor::SyncMessage {
                        header,
                        payload,
                        delivered_from: peer,
                        topic: topic.clone(),
                    })
                    .await
                    .map_err(|err| SyncError::Critical(format!("engine_actor_tx failed: {err}")))?;
            }

            engine_actor_tx
                .send(ToEngineActor::SyncDone { peer, topic })
                .await
                .map_err(|err| SyncError::Critical(format!("engine_actor_tx failed: {err}")))?;

            Ok(())
        })
    };

    // Run the "initiating peer" side of the sync protocol.
    sync_protocol
        .initiate(
            topic,
            Box::new(&mut send),
            Box::new(&mut recv),
            Box::new(&mut sink),
        )
        .await?;

    // We're expecting the task to exit with a result soon, we're awaiting it here ..
    let glue_task_result = glue_task_handle
        .await
        .map_err(|err| SyncError::Critical(format!("glue task handle failed: {err}")))?;

    // .. to inform some brave developer who will read the error logs ..
    if let Err(SyncError::Critical(err)) = &glue_task_result {
        error!("critical error in sync protocol: {err}");
    }

    // .. and forward it further!
    glue_task_result
}

/// Accept a sync protocol session over the provided bi-directional stream for the given peer and
/// topic.
pub async fn accept_sync<T, S, R>(
    mut send: &mut S,
    mut recv: &mut R,
    peer: PublicKey,
    sync_protocol: Arc<dyn for<'a> SyncProtocol<'a, T> + 'static>,
    engine_actor_tx: mpsc::Sender<ToEngineActor<T>>,
) -> Result<(), SyncError>
where
    T: Topic + TopicId + 'static,
    S: AsyncWrite + Send + Unpin,
    R: AsyncRead + Send + Unpin,
{
    debug!("accept sync session with peer {}", peer);

    // Set up a channel for receiving messages from the sync session.
    let (tx, mut rx) = mpsc::channel::<FromSync<T>>(128);
    let mut sink = PollSender::new(tx).sink_map_err(|e| SyncError::Critical(e.to_string()));

    // Set up a channel for sending over errors to the "glue" task which occurred during sync.
    let (sync_error_tx, mut sync_error_rx) = oneshot::channel::<SyncError>();

    // Spawn a "glue" task which represents the layer between the sync session and the engine.
    //
    // It picks up any messages from the sync session makes sure that the "Two-Phase Sync Flow" is
    // followed (I. "Handshake" Phase & II. "Data Sync" Phase) and the engine accordingly informed
    // about it.
    //
    // If the task detects any invalid behaviour of the sync flow, it fails critically, indicating
    // that the sync protocol implementation does not behave correctly and is not compatible with
    // the engine.
    //
    // Additionally, the task forwards any synced application data straight to the engine.
    let glue_task_handle: JoinHandle<Result<(), SyncError>> = tokio::spawn(async move {
        let mut topic = None;

        loop {
            tokio::select! {
                biased;

                Some(message) = rx.recv() => {
                    // I. Handshake Phase.
                    //
                    // At the beginning of every sync session the "accepting" peer needs to learn
                    // the topic of the "initiating" peer during the handshake phase. This is
                    // _always_ the first message we're expecting:
                    if let FromSync::HandshakeSuccess(handshake_topic) = message {
                        // It should only be sent once so topic should be `None` now.
                        if topic.is_some() {
                            return Err(SyncError::Critical("received handshake message twice from sync session in handshake phase".into()));
                        }

                        topic = Some(handshake_topic.clone());

                        // Inform the engine that we are expecting sync messages from the peer on
                        // this topic.
                        engine_actor_tx
                            .send(ToEngineActor::SyncHandshakeSuccess {
                                peer,
                                topic: handshake_topic,
                            })
                            .await
                            .map_err(|err| SyncError::Critical(format!("engine_actor_tx failed: {err}")))?;

                        continue;
                    }

                    // II. Data Sync Phase.
                    //
                    // At this stage we're beginning the actual "sync" protocol and expect messages
                    // containing the data which was received from the "initiating" peer.
                    //
                    // Please note that in not all sync implementations the "accepting" peers
                    // receives data.

                    // The topic must be known at this point in order to process further messages.
                    //
                    // Any sync protocol implementation should have already failed with an
                    // "unexpected behaviour" error if the topic wasn't learned. If this didn't
                    // happen (due to an incorrect implementation) we will critically fail now.
                    let Some(topic) = &topic else {
                        return Err(SyncError::Critical("never received handshake message from sync session in handshake phase".into()));
                    };

                    // From this point on we are only expecting "data" messages from the sync
                    // session.
                    let FromSync::Data(header, payload) = message else {
                        return Err(SyncError::Critical("expected to receive only data messages from sync session in data sync phase".into()));
                    };

                    engine_actor_tx
                        .send(ToEngineActor::SyncMessage {
                            header,
                            payload,
                            delivered_from: peer,
                            topic: topic.clone(),
                        })
                        .await
                        .map_err(|err| SyncError::Critical(format!("engine_actor_tx failed: {err}")))?;
                },
                Ok(err) = &mut sync_error_rx => {
                    engine_actor_tx
                        .send(ToEngineActor::SyncFailed {
                            peer,
                            topic: topic.clone(),
                        })
                        .await
                        .map_err(|err| SyncError::Critical(format!("engine_actor_tx failed: {err}")))?;

                    // If we're observing a critical error we terminate the task here and propagate
                    // that error further up.
                    //
                    // For any other error we're trusting the sync protocol implementation to
                    // properly wind down.
                    if let SyncError::Critical(err) = err {
                        return Err(SyncError::Critical(err));
                    } else {
                        return Ok(());
                    }
                },
                else => {
                    // Stream from sync session got terminated.
                    break;
                }
            }
        }

        // If topic was never set then we didn't receive any messages. In that case, the engine
        // wasn't ever informed, so we can return here silently.
        let Some(topic) = topic else {
            return Ok(());
        };

        engine_actor_tx
            .send(ToEngineActor::SyncDone { peer, topic })
            .await
            .map_err(|err| SyncError::Critical(format!("engine_actor_tx failed: {err}")))?;

        Ok(())
    });

    // Run the "accepting peer" side of the sync protocol.
    let result = sync_protocol
        .accept(
            Box::new(&mut send),
            Box::new(&mut recv),
            Box::new(&mut sink),
        )
        .await;

    // The sync protocol failed and we're informing the "glue" task about it, so it can accordingly
    // wind down and inform the engine about it.
    if let Err(sync_session_err) = result {
        sync_error_tx
            .send(sync_session_err)
            .map_err(|err| SyncError::Critical(format!("sync_error_tx failed: {err}")))?;
    }

    // We're expecting the task to exit with a result soon, we're awaiting it here ..
    let glue_task_result = glue_task_handle
        .await
        .map_err(|err| SyncError::Critical(format!("glue task handle failed: {err}")))?;

    // .. to inform some brave developer who will read the error logs ..
    if let Err(SyncError::Critical(err)) = &glue_task_result {
        error!("critical error in sync protocol: {err}");
    }

    // .. and forward it further!
    glue_task_result
}

#[cfg(test)]
mod sync_protocols {
    use std::sync::Arc;

    use async_trait::async_trait;
    use futures_lite::{AsyncRead, AsyncWrite, StreamExt};
    use futures_util::{Sink, SinkExt};
    use p2panda_sync::cbor::{into_cbor_sink, into_cbor_stream};
    use p2panda_sync::{FromSync, SyncError, SyncProtocol};
    use serde::{Deserialize, Serialize};
    use tracing::debug;

    use super::tests::TestTopic;

    #[derive(Debug, Serialize, Deserialize)]
    enum ProtocolMessage {
        Topic(TestTopic),
        Done,
    }

    /// A sync implementation which returns a mocked error.
    #[derive(Debug)]
    pub enum FailingProtocol {
        /// A critical error is triggered inside `accept()` after sync messages have been
        /// exchanged.
        AcceptorFailsCritical,

        /// A critical error is triggered inside `initiate()` after the handshake is complete.
        InitiatorFailsCritical,

        /// An critical error is triggered inside `accept()` by sending the topic twice from
        /// `initiate()`.
        InitiatorSendsTopicTwice,

        /// An critical error is triggered inside `initiate()` by sending a topic from `accept()`.
        AcceptorSendsTopic,

        /// No errors are explicitly triggered; used for "happy path" test.
        NoError,
    }

    #[async_trait]
    impl<'a> SyncProtocol<'a, TestTopic> for FailingProtocol {
        fn name(&self) -> &'static str {
            "failing-protocol"
        }

        async fn initiate(
            self: Arc<Self>,
            topic: TestTopic,
            tx: Box<&'a mut (dyn AsyncWrite + Send + Unpin)>,
            rx: Box<&'a mut (dyn AsyncRead + Send + Unpin)>,
            mut app_tx: Box<
                &'a mut (dyn Sink<FromSync<TestTopic>, Error = SyncError> + Send + Unpin),
            >,
        ) -> Result<(), SyncError> {
            let mut sink = into_cbor_sink(tx);
            let mut stream = into_cbor_stream(rx);

            sink.send(ProtocolMessage::Topic(topic.clone())).await?;

            // Simulate critical sync implementation bug by sending the topic a second time.
            if let FailingProtocol::InitiatorSendsTopicTwice = *self {
                sink.send(ProtocolMessage::Topic(topic.clone())).await?;
            }

            sink.send(ProtocolMessage::Done).await?;
            app_tx.send(FromSync::HandshakeSuccess(topic)).await?;

            // Simulate some critical error which occurred inside the sync session.
            if let FailingProtocol::InitiatorFailsCritical = *self {
                return Err(SyncError::Critical(
                    "something really bad happened in the initiator".to_string(),
                ));
            }

            while let Some(result) = stream.next().await {
                let message: ProtocolMessage = result?;
                match &message {
                    ProtocolMessage::Topic(_) => {
                        return Err(SyncError::UnexpectedBehaviour("initiator".to_string()));
                    }
                    ProtocolMessage::Done => break,
                }
            }

            Ok(())
        }

        async fn accept(
            self: Arc<Self>,
            tx: Box<&'a mut (dyn AsyncWrite + Send + Unpin)>,
            rx: Box<&'a mut (dyn AsyncRead + Send + Unpin)>,
            mut app_tx: Box<
                &'a mut (dyn Sink<FromSync<TestTopic>, Error = SyncError> + Send + Unpin),
            >,
        ) -> Result<(), SyncError> {
            let mut sink = into_cbor_sink(tx);
            let mut stream = into_cbor_stream(rx);

            // Simulate critical sync implementation bug by sending the topic from the acceptor (it
            // _never_ sends any topics).
            if let FailingProtocol::AcceptorSendsTopic = *self {
                let topic = TestTopic::new("unexpected behaviour test");
                sink.send(ProtocolMessage::Topic(topic)).await?;
            }

            let mut received_topic = false;

            while let Some(result) = stream.next().await {
                let message: ProtocolMessage = result?;
                debug!("message received: {:?}", message);

                match &message {
                    ProtocolMessage::Topic(topic) => {
                        if !received_topic {
                            app_tx
                                .send(FromSync::HandshakeSuccess(topic.clone()))
                                .await?;
                            received_topic = true;
                        } else {
                            return Err(SyncError::UnexpectedBehaviour("acceptor".to_string()));
                        }
                    }
                    ProtocolMessage::Done => break,
                }
            }

            if let FailingProtocol::AcceptorFailsCritical = *self {
                return Err(SyncError::Critical(
                    "something really bad happened in the acceptor".to_string(),
                ));
            }

            sink.send(ProtocolMessage::Done).await?;

            sink.flush().await?;
            app_tx.flush().await?;

            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use iroh_net::NodeId;
    use p2panda_core::{Hash, PrivateKey};
    use p2panda_sync::{SyncError, Topic};
    use serde::{Deserialize, Serialize};
    use tokio::sync::mpsc;
    use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

    use crate::engine::ToEngineActor;
    use crate::{sync, TopicId};

    use super::sync_protocols::FailingProtocol;

    #[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
    pub struct TestTopic(String, [u8; 32]);

    impl TestTopic {
        pub fn new(name: &str) -> Self {
            Self(name.to_owned(), *Hash::new(&name).as_bytes())
        }
    }

    impl Topic for TestTopic {}

    impl TopicId for TestTopic {
        fn id(&self) -> [u8; 32] {
            self.1
        }
    }

    async fn assert_sync_impl(protocol: FailingProtocol) {
        let peer_a = NodeId::from_bytes(PrivateKey::new().public_key().as_bytes()).unwrap();
        let peer_b = NodeId::from_bytes(PrivateKey::new().public_key().as_bytes()).unwrap();
        let topic = TestTopic::new("critical error test");

        let expect_error = match protocol {
            FailingProtocol::NoError => false,
            _ => true,
        };

        let sync_protocol = Arc::new(protocol);

        // Duplex streams which simulate both ends of a bi-directional network connection.
        let (peer_a_stream, peer_b_stream) = tokio::io::duplex(64 * 1024);
        let (peer_a_read, peer_a_write) = tokio::io::split(peer_a_stream);
        let (peer_b_read, peer_b_write) = tokio::io::split(peer_b_stream);

        // Channel for sending messages out of a running sync session.
        let (peer_a_app_tx, mut _peer_a_app_rx) = mpsc::channel(128);
        let (peer_b_app_tx, mut peer_b_app_rx) = mpsc::channel(128);

        let sync_protocol_clone = sync_protocol.clone();

        // Initiate a sync session.
        let initiator_handle = {
            let topic = topic.clone();

            tokio::spawn(async move {
                sync::initiate_sync(
                    &mut peer_a_write.compat_write(),
                    &mut peer_a_read.compat(),
                    peer_b,
                    topic.clone(),
                    sync_protocol,
                    peer_a_app_tx,
                )
                .await
            })
        };

        // Accept a sync session.
        //
        // A critical error will be triggered inside this method.
        let acceptor_result = sync::accept_sync(
            &mut peer_b_write.compat_write(),
            &mut peer_b_read.compat(),
            peer_a,
            sync_protocol_clone,
            peer_b_app_tx,
        )
        .await;

        let initiator_result = initiator_handle.await.unwrap();
        match expect_error {
            false => {
                assert!(acceptor_result.is_ok());
                assert!(initiator_result.is_ok());
            }
            true => {
                // We expect both "initiator" and "acceptor" methods to return the error.
                assert!(matches!(acceptor_result, Err(SyncError::Critical(_))));
                assert!(matches!(initiator_result, Err(SyncError::Critical(_))));
            }
        }

        // Ensure `SyncHandshakeSuccess` is being sent to the engine actor by the acceptor.
        let msg = peer_b_app_rx.recv().await.unwrap();
        let ToEngineActor::SyncHandshakeSuccess {
            topic: received_topic,
            peer,
        } = msg
        else {
            panic!("expected SyncHandshakeSuccess: {:?}", msg)
        };
        assert_eq!(received_topic, topic);
        assert_eq!(peer, peer_a);

        // Ensure `SyncFailed` is being sent to the engine actor by the acceptor.
        let msg = peer_b_app_rx.recv().await.unwrap();
        let ToEngineActor::SyncFailed {
            topic: received_topic,
            peer,
        } = msg
        else {
            panic!("expected SyncFailed: {:?}", msg)
        };
        assert_eq!(received_topic, Some(topic));
        assert_eq!(peer, peer_a);

        // Ensure no further messages are being sent to the engine actor by the acceptor.
        let msg = peer_b_app_rx.recv().await;
        assert!(msg.is_none());
    }

    #[tokio::test]
    async fn invalid_sync_protocol_impls() {
        assert_sync_impl(FailingProtocol::InitiatorFailsCritical).await;
        // assert_sync_impl(FailingProtocol::InitiatorSendsTopicTwice).await;
        // assert_sync_impl(FailingProtocol::AcceptorFailsCritical).await;
        // assert_sync_impl(FailingProtocol::AcceptorSendsTopic).await;
    }

    // #[tokio::test]
    // async fn run_sync_without_error() {
    //     assert_sync_impl(FailingProtocol::NoError).await;
    // }
}
