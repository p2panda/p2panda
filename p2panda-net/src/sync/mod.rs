// SPDX-License-Identifier: AGPL-3.0-or-later

mod handler;
pub(crate) mod manager;

pub use handler::{SyncConnection, SYNC_CONNECTION_ALPN};

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
        .expect("engine channel closed");

    // Set up a channel for receiving new application messages.
    let (tx, mut rx) = mpsc::channel::<FromSync<T>>(128);
    let mut sink = PollSender::new(tx).sink_map_err(|e| SyncError::Critical(e.to_string()));

    // Spawn a task which picks up any new application messages and sends them on to the engine
    // for handling.
    {
        let engine_actor_tx = engine_actor_tx.clone();
        let mut sync_handshake_success = false;
        let topic = topic.clone();

        tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                // 1. Handshake Phase.
                // ~~~~~~~~~~~~~~~~~~~
                //
                // At the beginning of every sync session the "initiating" peer needs to send over
                // the topic to the "accepting" peer during the handshake phase. This is the first
                // message we're expecting:
                if let FromSync::HandshakeSuccess(_) = &message {
                    // Receiving the handshake message twice is a protocol violation.
                    if sync_handshake_success {
                        // @TODO(glyph): We are failing silently here. Consider propagating the error
                        // or informing the engine actor directly.
                        error!("received handshake twice from peer {}", peer);
                        break;
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
                        .expect("engine channel closed");

                    continue;
                }

                // 2. Data Sync Phase.
                // ~~~~~~~~~~~~~~~~~~~
                let FromSync::Data(header, payload) = message else {
                    // @TODO(glyph): We are failing silently here. Consider propagating the error
                    // or informing the engine actor directly.
                    error!("expected message bytes after handshake from peer {}", peer);
                    return;
                };

                if let Err(err) = engine_actor_tx
                    .send(ToEngineActor::SyncMessage {
                        header,
                        payload,
                        delivered_from: peer,
                        topic: topic.clone(),
                    })
                    .await
                {
                    error!("error in sync actor: {}", err)
                };
            }

            engine_actor_tx
                .send(ToEngineActor::SyncDone { peer, topic })
                .await
                .expect("engine channel closed");
        });
    }

    // Run the sync protocol.
    //
    // When an error happens while _accepting_ a sync session (as in `accept_sync()` below) we
    // simply notify the engine actor directly, since the acceptor does not need to track
    // reattempts.
    sync_protocol
        .initiate(
            topic.clone(),
            Box::new(&mut send),
            Box::new(&mut recv),
            Box::new(&mut sink),
        )
        .await?;

    Ok(())
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

    // Set up a channel for receiving new application messages.
    let (tx, mut rx) = mpsc::channel::<FromSync<T>>(128);
    let mut sink = PollSender::new(tx).sink_map_err(|e| SyncError::Critical(e.to_string()));

    // Set up a channel for sending over errors to the task which happened during sync.
    let (sync_error_tx, mut sync_error_rx) = oneshot::channel::<SyncError>();

    // Spawn a task which picks up any new application messages and sends them on to the engine
    // for handling.
    tokio::spawn(async move {
        let mut topic = None;

        loop {
            tokio::select! {
                biased;
                Some(message) = rx.recv() => {
                    // 1. Handshake Phase.
                    // ~~~~~~~~~~~~~~~~~~~
                    //
                    // At the beginning of every sync session the "accepting" peer needs to learn the
                    // topic of the "initiating" peer during the handshake phase. This is the first
                    // message we're expecting:
                    if let FromSync::HandshakeSuccess(handshake_topic) = &message {
                        // It should only be sent once so topic should be `None` now.
                        if topic.is_some() {
                            // @TODO(glyph): In the future we should either notify the engine actor
                            // of this failure directly or return the error to the upstream caller.
                            error!("sync protocol violation: received handshake message twice from peer {}", peer);
                            break;
                        }

                        topic = Some(handshake_topic.clone());

                        // Inform the engine that we are expecting sync messages from the peer on
                        // this topic.
                        engine_actor_tx
                            .send(ToEngineActor::SyncHandshakeSuccess {
                                peer,
                                topic: handshake_topic.clone(),
                            })
                            .await
                            .expect("engine channel closed");

                        continue;
                    }

                    // 2. Data Sync Phase.
                    // ~~~~~~~~~~~~~~~~~~~
                    //
                    // The topic must be known at this point in order to process further messages.
                    let Some(topic) = &topic else {
                        error!("sync protocol violation: topic not received from peer {}", peer);
                        return;
                    };

                    let FromSync::Data(header, payload) = message else {
                        // @TODO(glyph): In the future we should either notify the engine actor
                        // of this failure directly or return the error to the upstream caller.
                        error!("sync protocol violation: expected message bytes from peer {}", peer);
                        return;
                    };

                    engine_actor_tx
                        .send(ToEngineActor::SyncMessage {
                            header,
                            payload,
                            delivered_from: peer,
                            topic: topic.clone(),
                        })
                        .await
                        .expect("engine channel closed");
                },
                Ok(_) = &mut sync_error_rx => {
                    engine_actor_tx
                        .send(ToEngineActor::SyncFailed {
                            peer,
                            topic: topic.clone(),
                        })
                        .await
                        .expect("engine channel closed");
                    return;
                },
                else => {
                    break;
                }
            }
        }

        // If topic was never set then we didn't receive any messages. In that case, the engine
        // wasn't informed it should buffer messages and so we can return here.
        let Some(topic) = topic else {
            return;
        };

        engine_actor_tx
            .send(ToEngineActor::SyncDone { peer, topic })
            .await
            .expect("engine channel closed");
    });

    // Run the sync protocol.
    let result = sync_protocol
        .accept(
            Box::new(&mut send),
            Box::new(&mut recv),
            Box::new(&mut sink),
        )
        .await;

    if let Err(sync_err) = result {
        sync_error_tx
            .send(sync_err)
            .expect("error oneshot sender failed");
    }

    Ok(())
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

    /// A sync implementation which returns an error.
    #[derive(Debug)]
    pub enum Protocol {
        AcceptCritical,
        InitiateCritical,
    }

    #[async_trait]
    impl<'a> SyncProtocol<'a, TestTopic> for Protocol {
        fn name(&self) -> &'static str {
            static PROTOCOL_NAME: &str = "error_protocol";
            PROTOCOL_NAME
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
            debug!("initiate sync session");

            let mut sink = into_cbor_sink(tx);
            let mut stream = into_cbor_stream(rx);

            sink.send(ProtocolMessage::Topic(topic.clone())).await?;
            sink.send(ProtocolMessage::Done).await?;
            app_tx.send(FromSync::HandshakeSuccess(topic)).await?;

            if let Protocol::InitiateCritical = *self {
                return Err(SyncError::Critical("initiator".to_string()));
            }

            while let Some(result) = stream.next().await {
                let message: ProtocolMessage = result?;
                debug!("message received: {:?}", message);

                match &message {
                    ProtocolMessage::Topic(_) => panic!(),
                    ProtocolMessage::Done => break,
                }
            }

            sink.flush().await?;
            app_tx.flush().await?;

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
            debug!("accept sync session");

            let mut sink = into_cbor_sink(tx);
            let mut stream = into_cbor_stream(rx);

            while let Some(result) = stream.next().await {
                let message: ProtocolMessage = result?;
                debug!("message received: {:?}", message);

                match &message {
                    ProtocolMessage::Topic(topic) => {
                        app_tx
                            .send(FromSync::HandshakeSuccess(topic.clone()))
                            .await?;
                    }
                    ProtocolMessage::Done => break,
                }
            }

            if let Protocol::AcceptCritical = *self {
                return Err(SyncError::Critical("acceptor".to_string()));
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
    use p2panda_core::PrivateKey;
    use p2panda_sync::{SyncError, Topic};
    use serde::{Deserialize, Serialize};
    use tokio::sync::mpsc;
    use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

    use crate::engine::ToEngineActor;
    use crate::{sync, TopicId};

    use super::sync_protocols::Protocol;

    #[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
    pub struct TestTopic(String, [u8; 32]);

    impl TestTopic {
        pub fn new(name: &str) -> Self {
            Self(name.to_owned(), [0; 32])
        }
    }

    impl Topic for TestTopic {}

    impl TopicId for TestTopic {
        fn id(&self) -> [u8; 32] {
            self.1
        }
    }

    #[tokio::test]
    async fn accept_sync_with_critical_error() {
        let peer_a = NodeId::from_bytes(PrivateKey::new().public_key().as_bytes()).unwrap();
        let peer_b = NodeId::from_bytes(PrivateKey::new().public_key().as_bytes()).unwrap();
        let topic = TestTopic::new("critical error test");
        let sync_protocol = Arc::new(Protocol::AcceptCritical);

        // Duplex streams which simulate both ends of a bi-directional network connection.
        let (peer_a_stream, peer_b_stream) = tokio::io::duplex(64 * 1024);
        let (peer_a_read, peer_a_write) = tokio::io::split(peer_a_stream);
        let (peer_b_read, peer_b_write) = tokio::io::split(peer_b_stream);

        // Channel for sending messages out of a running sync session.
        let (peer_a_app_tx, mut _peer_a_app_rx) = mpsc::channel(128);
        let (peer_b_app_tx, mut peer_b_app_rx) = mpsc::channel(128);

        let sync_protocol_clone = sync_protocol.clone();

        // Initiate a sync session.
        {
            let topic = topic.clone();

            let _initiate_handle = tokio::spawn(async move {
                sync::initiate_sync(
                    &mut peer_a_write.compat_write(),
                    &mut peer_a_read.compat(),
                    peer_b,
                    topic.clone(),
                    sync_protocol,
                    peer_a_app_tx,
                )
                .await
            });
        }

        // Accept a sync session.
        //
        // A critical error will be triggered inside this method.
        let result = sync::accept_sync(
            &mut peer_b_write.compat_write(),
            &mut peer_b_read.compat(),
            peer_a,
            sync_protocol_clone,
            peer_b_app_tx,
        )
        .await;

        // The error is caught inside `accept_sync()` and reported directly to the engine.
        assert!(result.is_ok());

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
    async fn initiate_sync_with_critical_error() {
        let peer_a = NodeId::from_bytes(PrivateKey::new().public_key().as_bytes()).unwrap();
        let peer_b = NodeId::from_bytes(PrivateKey::new().public_key().as_bytes()).unwrap();
        let topic = TestTopic::new("critical error test");
        let sync_protocol = Arc::new(Protocol::InitiateCritical);

        // Duplex streams which simulate both ends of a bi-directional network connection.
        let (peer_a_stream, peer_b_stream) = tokio::io::duplex(64 * 1024);
        let (peer_a_read, peer_a_write) = tokio::io::split(peer_a_stream);
        let (peer_b_read, peer_b_write) = tokio::io::split(peer_b_stream);

        // Channel for sending messages out of a running sync session.
        let (peer_a_app_tx, mut peer_a_app_rx) = mpsc::channel(128);
        let (peer_b_app_tx, mut _peer_b_app_rx) = mpsc::channel(128);

        // Accept a sync session.
        {
            let sync_protocol = sync_protocol.clone();

            let _accept_handle = tokio::spawn(async move {
                sync::accept_sync(
                    &mut peer_b_write.compat_write(),
                    &mut peer_b_read.compat(),
                    peer_a,
                    sync_protocol,
                    peer_b_app_tx,
                )
                .await
            });
        }

        // Initiate a sync session.
        //
        // A critical error will be triggered inside this method.
        let result = sync::initiate_sync(
            &mut peer_a_write.compat_write(),
            &mut peer_a_read.compat(),
            peer_b,
            topic.clone(),
            sync_protocol,
            peer_a_app_tx,
        )
        .await;

        assert_eq!(result, Err(SyncError::Critical("initiator".to_string())));

        // Ensure `SyncStart` is being sent to the engine actor by the initiator.
        let msg = peer_a_app_rx.recv().await.unwrap();
        let ToEngineActor::SyncStart {
            topic: received_topic,
            peer,
        } = msg
        else {
            panic!("expected SyncStart: {:?}", msg)
        };
        assert_eq!(received_topic, topic);
        assert_eq!(peer, peer_b);

        // Ensure `SyncHandshakeSuccess` is being sent to the engine actor by the initiator.
        let msg = peer_a_app_rx.recv().await.unwrap();
        let ToEngineActor::SyncHandshakeSuccess {
            topic: received_topic,
            peer,
        } = msg
        else {
            panic!("expected SyncHandshakeSuccess: {:?}", msg)
        };
        assert_eq!(received_topic, topic);
        assert_eq!(peer, peer_b);

        // Ensure `SyncDone` is being sent to the engine actor by the initiator.
        let msg = peer_a_app_rx.recv().await.unwrap();
        let ToEngineActor::SyncDone {
            topic: received_topic,
            peer,
        } = msg
        else {
            panic!("expected SyncDone: {:?}", msg)
        };
        assert_eq!(received_topic, topic);
        assert_eq!(peer, peer_b);

        // Ensure no further messages are being sent to the engine actor by the initiator.
        let msg = peer_a_app_rx.recv().await;
        assert!(msg.is_none());
    }
}
