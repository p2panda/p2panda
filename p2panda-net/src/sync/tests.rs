// SPDX-License-Identifier: AGPL-3.0-or-later

mod sync_protocols {
    use std::sync::Arc;

    use async_trait::async_trait;
    use futures_lite::{AsyncRead, AsyncWrite, StreamExt};
    use futures_util::{Sink, SinkExt};
    use p2panda_sync::cbor::{into_cbor_sink, into_cbor_stream};
    use p2panda_sync::{FromSync, SyncError, SyncProtocol};
    use serde::{Deserialize, Serialize};

    use super::TestTopic;

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

        /// An unexpected behaviour error is triggered inside `accept()` by sending the topic twice
        /// from `initiate()`.
        InitiatorSendsTopicTwice,

        /// An unexpected behaviour error is triggered inside `initiate()` by sending a topic from
        /// `accept()`.
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

            // Simulate some critical error which occurred inside the sync session.
            if let FailingProtocol::InitiatorFailsCritical = *self {
                return Err(SyncError::Critical(
                    "something really bad happened in the initiator".to_string(),
                ));
            }

            sink.send(ProtocolMessage::Done).await?;

            app_tx.send(FromSync::HandshakeSuccess(topic)).await?;

            while let Some(result) = stream.next().await {
                let message: ProtocolMessage = result?;
                match &message {
                    ProtocolMessage::Topic(_) => {
                        return Err(SyncError::UnexpectedBehaviour(
                            "unexpected message received from acceptor".to_string(),
                        ));
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
                match &message {
                    ProtocolMessage::Topic(topic) => {
                        if !received_topic {
                            app_tx
                                .send(FromSync::HandshakeSuccess(topic.clone()))
                                .await?;
                            received_topic = true;
                        } else {
                            return Err(SyncError::UnexpectedBehaviour(
                                "received topic too often".to_string(),
                            ));
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

            Ok(())
        }
    }
}

use std::sync::Arc;

use futures_util::FutureExt;
use iroh_net::NodeId;
use p2panda_core::{Hash, PrivateKey};
use p2panda_sync::{SyncError, Topic};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::engine::ToEngineActor;
use crate::{sync, TopicId};

use sync_protocols::FailingProtocol;

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

async fn run_sync_impl(
    protocol: FailingProtocol,
) -> (
    mpsc::Receiver<ToEngineActor<TestTopic>>,
    mpsc::Receiver<ToEngineActor<TestTopic>>,
    JoinHandle<Result<(), SyncError>>,
    JoinHandle<Result<(), SyncError>>,
) {
    let topic = TestTopic::new("run test protocol impl");

    let initiator_node_id = NodeId::from_bytes(PrivateKey::new().public_key().as_bytes()).unwrap();
    let acceptor_node_id = NodeId::from_bytes(PrivateKey::new().public_key().as_bytes()).unwrap();

    let sync_protocol = Arc::new(protocol);

    // Duplex streams which simulate both ends of a bi-directional network connection.
    let (initiator_stream, acceptor_stream) = tokio::io::duplex(64 * 1024);
    let (initiator_read, initiator_write) = tokio::io::split(initiator_stream);
    let (acceptor_read, acceptor_write) = tokio::io::split(acceptor_stream);

    // Channel for sending messages out of a running sync session.
    let (initiator_tx, initiator_rx) = mpsc::channel(128);
    let (acceptor_tx, acceptor_rx) = mpsc::channel(128);

    let sync_protocol_clone = sync_protocol.clone();

    let initiator_handle = {
        let topic = topic.clone();

        tokio::spawn(async move {
            sync::initiate_sync(
                &mut initiator_write.compat_write(),
                &mut initiator_read.compat(),
                acceptor_node_id,
                topic.clone(),
                sync_protocol,
                initiator_tx,
            )
            .await
        })
    };

    let acceptor_handle = {
        tokio::spawn(async move {
            sync::accept_sync(
                &mut acceptor_write.compat_write(),
                &mut acceptor_read.compat(),
                initiator_node_id,
                sync_protocol_clone,
                acceptor_tx,
            )
            .await
        })
    };

    (initiator_rx, acceptor_rx, initiator_handle, acceptor_handle)
}

#[tokio::test]
async fn initiator_fails_critical() {
    let (mut rx_initiator, mut rx_acceptor, initiator_handle, acceptor_handle) =
        run_sync_impl(FailingProtocol::InitiatorFailsCritical).await;

    // Expected initiator messages.
    assert!(matches!(
        rx_initiator.recv().await,
        Some(ToEngineActor::SyncStart { .. })
    ));

    // Note: "SyncFailed" message is handled by manager for initiators.
    assert!(rx_initiator.recv().now_or_never().unwrap().is_none());

    // Expected acceptor messages.
    assert!(matches!(
        rx_acceptor.recv().await,
        Some(ToEngineActor::SyncStart { .. })
    ));

    assert!(matches!(
        rx_acceptor.recv().await,
        Some(ToEngineActor::SyncHandshakeSuccess { .. })
    ));

    assert!(matches!(
        rx_acceptor.recv().await,
        Some(ToEngineActor::SyncFailed { .. })
    ));

    // Expected handler results.
    assert_eq!(
        initiator_handle.await.unwrap(),
        Err(SyncError::Critical(
            "something really bad happened in the initiator".into(),
        ))
    );
    assert_eq!(
        acceptor_handle.await.unwrap(),
        // @TODO: This error happens because the CBOR codec failed with the broken pipe to the
        // initiator end.
        //
        // This is a little bit confusing and should rather fail as an "connection error". On top
        // it's not a system critical one.
        Err(SyncError::Critical(
            "internal i/o stream error broken pipe".into()
        ))
    );
}

#[tokio::test]
async fn initiator_sends_topic_twice() {
    let (mut rx_initiator, mut rx_acceptor, initiator_handle, acceptor_handle) =
        run_sync_impl(FailingProtocol::InitiatorSendsTopicTwice).await;

    // Expected initiator messages.
    assert!(matches!(
        rx_initiator.recv().await,
        Some(ToEngineActor::SyncStart { .. })
    ));

    assert!(matches!(
        rx_initiator.recv().await,
        Some(ToEngineActor::SyncHandshakeSuccess { .. })
    ));

    assert!(matches!(
        rx_initiator.recv().await,
        Some(ToEngineActor::SyncDone { .. })
    ));

    // Expected acceptor messages.
    assert!(matches!(
        rx_acceptor.recv().await,
        Some(ToEngineActor::SyncStart { .. })
    ));

    assert!(matches!(
        rx_acceptor.recv().await,
        Some(ToEngineActor::SyncFailed { .. })
    ));

    assert_eq!(initiator_handle.await.unwrap(), Ok(()));
    assert_eq!(
        acceptor_handle.await.unwrap(),
        // This is _not_ a critical error as the acceptor protocol implementation handled the
        // protocol violation (sending topic twice) by itself.
        Err(SyncError::UnexpectedBehaviour(
            "received topic too often".into(),
        ))
    );
}

#[tokio::test]
async fn acceptor_fails_critical() {
    let (mut rx_initiator, mut rx_acceptor, initiator_handle, acceptor_handle) =
        run_sync_impl(FailingProtocol::AcceptorFailsCritical).await;

    // Expected initiator messages.
    assert!(matches!(
        rx_initiator.recv().await,
        Some(ToEngineActor::SyncStart { .. })
    ));

    assert!(matches!(
        rx_initiator.recv().await,
        Some(ToEngineActor::SyncHandshakeSuccess { .. })
    ));

    assert!(matches!(
        rx_initiator.recv().await,
        // Initiator can end the session without any problems as the acceptor failed at the end of
        // the protocol _after_ sending all important messages already to the initiator.
        Some(ToEngineActor::SyncDone { .. })
    ));

    // Expected acceptor messages.
    assert!(matches!(
        rx_acceptor.recv().await,
        Some(ToEngineActor::SyncStart { .. })
    ));

    assert!(matches!(
        rx_acceptor.recv().await,
        Some(ToEngineActor::SyncFailed { .. })
    ));

    // Expected handler results.
    assert_eq!(initiator_handle.await.unwrap(), Ok(()));
    assert_eq!(
        acceptor_handle.await.unwrap(),
        Err(SyncError::Critical(
            "something really bad happened in the acceptor".into(),
        ))
    );
}

#[tokio::test]
async fn acceptor_sends_topic() {
    let (mut rx_initiator, mut rx_acceptor, initiator_handle, acceptor_handle) =
        run_sync_impl(FailingProtocol::AcceptorSendsTopic).await;

    // Expected initiator messages.
    assert!(matches!(
        rx_initiator.recv().await,
        Some(ToEngineActor::SyncStart { .. })
    ));

    assert!(matches!(
        rx_initiator.recv().await,
        Some(ToEngineActor::SyncHandshakeSuccess { .. })
    ));

    // Note: "SyncFailed" message is handled by manager for initiators.
    assert!(rx_initiator.recv().now_or_never().unwrap().is_none());

    // Expected acceptor messages.
    assert!(matches!(
        rx_acceptor.recv().await,
        Some(ToEngineActor::SyncStart { .. })
    ));

    assert!(matches!(
        rx_acceptor.recv().await,
        Some(ToEngineActor::SyncHandshakeSuccess { .. })
    ));

    assert!(matches!(
        rx_acceptor.recv().await,
        Some(ToEngineActor::SyncDone { .. })
    ));

    // Expected handler results.
    assert_eq!(
        initiator_handle.await.unwrap(),
        Err(SyncError::UnexpectedBehaviour(
            "unexpected message received from acceptor".into(),
        ))
    );
    assert_eq!(acceptor_handle.await.unwrap(), Ok(()));
}

#[tokio::test]
async fn run_sync_without_error() {
    let (mut rx_initiator, mut rx_acceptor, initiator_handle, acceptor_handle) =
        run_sync_impl(FailingProtocol::NoError).await;

    // Expected initiator messages.
    assert!(matches!(
        rx_initiator.recv().await,
        Some(ToEngineActor::SyncStart { .. })
    ));

    assert!(matches!(
        rx_initiator.recv().await,
        Some(ToEngineActor::SyncHandshakeSuccess { .. })
    ));

    assert!(matches!(
        rx_initiator.recv().await,
        Some(ToEngineActor::SyncDone { .. })
    ));

    // Expected acceptor messages.
    assert!(matches!(
        rx_acceptor.recv().await,
        Some(ToEngineActor::SyncStart { .. })
    ));

    assert!(matches!(
        rx_acceptor.recv().await,
        Some(ToEngineActor::SyncHandshakeSuccess { .. })
    ));

    assert!(matches!(
        rx_acceptor.recv().await,
        Some(ToEngineActor::SyncDone { .. })
    ));

    // Expected handler results.
    assert_eq!(initiator_handle.await.unwrap(), Ok(()));
    assert_eq!(acceptor_handle.await.unwrap(), Ok(()));
}
