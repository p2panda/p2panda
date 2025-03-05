//! Sync protocol implementations for testing purposes.
//!
//! - `DummyProtocol`
//! - `PingPongProtocol`
//! - `FailingProtocol`
use std::sync::Arc;

use async_trait::async_trait;
use futures_lite::{AsyncRead, AsyncWrite, StreamExt};
use futures_util::{Sink, SinkExt};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::cbor::{into_cbor_sink, into_cbor_stream};
use crate::{FromSync, SyncError, SyncProtocol, TopicQuery};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct SyncTestTopic(String, pub [u8; 32]);

impl SyncTestTopic {
    pub fn new(name: &str) -> Self {
        Self(name.to_owned(), [0; 32])
    }
}

impl TopicQuery for SyncTestTopic {}

#[derive(Debug, Serialize, Deserialize)]
enum DummyProtocolMessage {
    TopicQuery(SyncTestTopic),
    Done,
}

/// A sync implementation which fulfills basic protocol requirements but nothing more
#[derive(Debug)]
pub struct DummyProtocol {}

#[async_trait]
impl<'a> SyncProtocol<'a, SyncTestTopic> for DummyProtocol {
    fn name(&self) -> &'static str {
        static DUMMY_PROTOCOL_NAME: &str = "dummy_protocol";
        DUMMY_PROTOCOL_NAME
    }

    async fn initiate(
        self: Arc<Self>,
        topic_query: SyncTestTopic,
        tx: Box<&'a mut (dyn AsyncWrite + Send + Unpin)>,
        rx: Box<&'a mut (dyn AsyncRead + Send + Unpin)>,
        mut app_tx: Box<
            &'a mut (dyn Sink<FromSync<SyncTestTopic>, Error = SyncError> + Send + Unpin),
        >,
    ) -> Result<(), SyncError> {
        debug!("DummyProtocol: initiate sync session");

        let mut sink = into_cbor_sink(tx);
        let mut stream = into_cbor_stream(rx);

        sink.send(DummyProtocolMessage::TopicQuery(topic_query.clone()))
            .await?;
        sink.send(DummyProtocolMessage::Done).await?;
        app_tx.send(FromSync::HandshakeSuccess(topic_query)).await?;

        while let Some(result) = stream.next().await {
            let message: DummyProtocolMessage = result?;
            debug!("message received: {:?}", message);

            match &message {
                DummyProtocolMessage::TopicQuery(_) => panic!(),
                DummyProtocolMessage::Done => break,
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
            &'a mut (dyn Sink<FromSync<SyncTestTopic>, Error = SyncError> + Send + Unpin),
        >,
    ) -> Result<(), SyncError> {
        debug!("DummyProtocol: accept sync session");

        let mut sink = into_cbor_sink(tx);
        let mut stream = into_cbor_stream(rx);

        while let Some(result) = stream.next().await {
            let message: DummyProtocolMessage = result?;
            debug!("message received: {:?}", message);

            match &message {
                DummyProtocolMessage::TopicQuery(topic_query) => {
                    app_tx
                        .send(FromSync::HandshakeSuccess(topic_query.clone()))
                        .await?
                }
                DummyProtocolMessage::Done => break,
            }
        }

        sink.send(DummyProtocolMessage::Done).await?;

        sink.flush().await?;
        app_tx.flush().await?;

        Ok(())
    }
}

// The protocol message types.
#[derive(Serialize, Deserialize)]
enum PingPongProtocolMessage {
    TopicQuery(SyncTestTopic),
    Ping,
    Pong,
}

/// A sync implementation where the initiator sends a `ping` message and the acceptor responds with
/// a `pong` message.
#[derive(Debug, Clone)]
pub struct PingPongProtocol {}

#[async_trait]
impl<'a> SyncProtocol<'a, SyncTestTopic> for PingPongProtocol {
    fn name(&self) -> &'static str {
        static SIMPLE_PROTOCOL_NAME: &str = "simple_protocol";
        SIMPLE_PROTOCOL_NAME
    }

    async fn initiate(
        self: Arc<Self>,
        topic_query: SyncTestTopic,
        tx: Box<&'a mut (dyn AsyncWrite + Send + Unpin)>,
        rx: Box<&'a mut (dyn AsyncRead + Send + Unpin)>,
        mut app_tx: Box<
            &'a mut (dyn Sink<FromSync<SyncTestTopic>, Error = SyncError> + Send + Unpin),
        >,
    ) -> Result<(), SyncError> {
        debug!("initiate sync session");
        let mut sink = into_cbor_sink(tx);
        let mut stream = into_cbor_stream(rx);

        sink.send(PingPongProtocolMessage::TopicQuery(topic_query.clone()))
            .await?;
        sink.send(PingPongProtocolMessage::Ping).await?;
        debug!("ping message sent");

        app_tx.send(FromSync::HandshakeSuccess(topic_query)).await?;

        while let Some(result) = stream.next().await {
            let message = result?;

            match message {
                PingPongProtocolMessage::TopicQuery(_) => panic!(),
                PingPongProtocolMessage::Ping => {
                    return Err(SyncError::UnexpectedBehaviour(
                        "unexpected Ping message received".to_string(),
                    ));
                }
                PingPongProtocolMessage::Pong => {
                    debug!("pong message received");
                    app_tx
                        .send(FromSync::Data {
                            header: "PONG".as_bytes().to_owned(),
                            payload: None,
                        })
                        .await
                        .unwrap();
                    break;
                }
            }
        }

        // Flush all bytes so that no messages are lost.
        sink.flush().await?;
        app_tx.flush().await?;

        Ok(())
    }

    async fn accept(
        self: Arc<Self>,
        tx: Box<&'a mut (dyn AsyncWrite + Send + Unpin)>,
        rx: Box<&'a mut (dyn AsyncRead + Send + Unpin)>,
        mut app_tx: Box<
            &'a mut (dyn Sink<FromSync<SyncTestTopic>, Error = SyncError> + Send + Unpin),
        >,
    ) -> Result<(), SyncError> {
        debug!("accept sync session");
        let mut sink = into_cbor_sink(tx);
        let mut stream = into_cbor_stream(rx);

        while let Some(result) = stream.next().await {
            let message = result?;

            match message {
                PingPongProtocolMessage::TopicQuery(topic_query) => {
                    app_tx.send(FromSync::HandshakeSuccess(topic_query)).await?
                }
                PingPongProtocolMessage::Ping => {
                    debug!("ping message received");
                    app_tx
                        .send(FromSync::Data {
                            header: "PING".as_bytes().to_owned(),
                            payload: None,
                        })
                        .await
                        .unwrap();

                    sink.send(PingPongProtocolMessage::Pong).await?;
                    debug!("pong message sent");
                    break;
                }
                PingPongProtocolMessage::Pong => {
                    return Err(SyncError::UnexpectedBehaviour(
                        "unexpected Pong message received".to_string(),
                    ));
                }
            }
        }

        sink.flush().await?;
        app_tx.flush().await?;

        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
enum FailingProtocolMessage {
    TopicQuery(SyncTestTopic),
    Done,
}

/// A sync implementation which returns a mocked error.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum FailingProtocol {
    /// A critical error is triggered inside `accept()` after sync messages have been
    /// exchanged.
    AcceptorFailsCritical,

    /// A critical error is triggered inside `initiate()` after the handshake is complete.
    InitiatorFailsCritical,

    /// An unexpected behaviour error is triggered inside `initiate()` after the topic query has
    /// been sent.
    InitiatorFailsUnexpected,

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
impl<'a> SyncProtocol<'a, SyncTestTopic> for FailingProtocol {
    fn name(&self) -> &'static str {
        "failing-protocol"
    }

    async fn initiate(
        self: Arc<Self>,
        topic: SyncTestTopic,
        tx: Box<&'a mut (dyn AsyncWrite + Send + Unpin)>,
        rx: Box<&'a mut (dyn AsyncRead + Send + Unpin)>,
        mut app_tx: Box<
            &'a mut (dyn Sink<FromSync<SyncTestTopic>, Error = SyncError> + Send + Unpin),
        >,
    ) -> Result<(), SyncError> {
        let mut sink = into_cbor_sink(tx);
        let mut stream = into_cbor_stream(rx);

        sink.send(FailingProtocolMessage::TopicQuery(topic.clone()))
            .await?;

        // Simulate critical sync implementation bug by sending the topic a second time.
        if let FailingProtocol::InitiatorSendsTopicTwice = *self {
            sink.send(FailingProtocolMessage::TopicQuery(topic.clone()))
                .await?;
        }

        // Simulate some critical error which occurred inside the sync session.
        if let FailingProtocol::InitiatorFailsCritical = *self {
            return Err(SyncError::Critical(
                "something really bad happened in the initiator".to_string(),
            ));
        }

        // Simulate unexpected behaviour (such as a broken pipe due to disconnection).
        if let FailingProtocol::InitiatorFailsUnexpected = *self {
            return Err(SyncError::UnexpectedBehaviour("bang!".to_string()));
        }

        sink.send(FailingProtocolMessage::Done).await?;

        app_tx.send(FromSync::HandshakeSuccess(topic)).await?;

        while let Some(result) = stream.next().await {
            let message: FailingProtocolMessage = result?;
            match &message {
                FailingProtocolMessage::TopicQuery(_) => {
                    return Err(SyncError::UnexpectedBehaviour(
                        "unexpected message received from acceptor".to_string(),
                    ));
                }
                FailingProtocolMessage::Done => break,
            }
        }

        Ok(())
    }

    async fn accept(
        self: Arc<Self>,
        tx: Box<&'a mut (dyn AsyncWrite + Send + Unpin)>,
        rx: Box<&'a mut (dyn AsyncRead + Send + Unpin)>,
        mut app_tx: Box<
            &'a mut (dyn Sink<FromSync<SyncTestTopic>, Error = SyncError> + Send + Unpin),
        >,
    ) -> Result<(), SyncError> {
        // Simulate some critical error which occurred inside the sync session.
        if let FailingProtocol::AcceptorFailsCritical = *self {
            return Err(SyncError::Critical(
                "something really bad happened in the acceptor".to_string(),
            ));
        }

        let mut sink = into_cbor_sink(tx);
        let mut stream = into_cbor_stream(rx);

        // Simulate critical sync implementation bug by sending the topic from the acceptor (it
        // _never_ sends any topics).
        if let FailingProtocol::AcceptorSendsTopic = *self {
            let topic = SyncTestTopic::new("unexpected behaviour test");
            sink.send(FailingProtocolMessage::TopicQuery(topic)).await?;
        }

        let mut received_topic = false;

        while let Some(result) = stream.next().await {
            let message: FailingProtocolMessage = result?;
            match &message {
                FailingProtocolMessage::TopicQuery(topic) => {
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
                FailingProtocolMessage::Done => break,
            }
        }

        sink.send(FailingProtocolMessage::Done).await?;

        Ok(())
    }
}
