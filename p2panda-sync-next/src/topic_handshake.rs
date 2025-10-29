use futures::channel::mpsc;
use futures::{Sink, Stream};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::marker::PhantomData;
use thiserror::Error;

use crate::traits::Protocol;

/// Initiator side of the topic handshake protocol.
///
/// After the protocol is complete both peers know the T of the initiator.
pub struct TopicHandshakeInitiator<T, Evt> {
    pub topic: T,
    pub event_tx: mpsc::Sender<Evt>,
}

impl<T, Evt> TopicHandshakeInitiator<T, Evt>
where
    T: Clone + for<'de> Deserialize<'de> + Serialize,
    Evt: From<TopicHandshakeEvent<T>>,
{
    pub fn new(topic: T, event_tx: mpsc::Sender<Evt>) -> Self {
        Self { topic, event_tx }
    }
}

impl<T, Evt> Protocol for TopicHandshakeInitiator<T, Evt>
where
    T: Clone + Debug + for<'de> Deserialize<'de> + Serialize,
    Evt: From<TopicHandshakeEvent<T>>,
{
    type Error = TopicHandshakeError<T>;
    type Output = ();
    type Message = TopicHandshakeMessage<T>;

    async fn run(
        mut self,
        sink: &mut (impl Sink<Self::Message, Error = Self::Error> + Unpin),
        stream: &mut (impl Stream<Item = Result<Self::Message, Self::Error>> + Unpin),
    ) -> Result<(), Self::Error> {
        // Announce that the topic handshake session has been initiated.
        self.event_tx
            .send(TopicHandshakeEvent::Initiate(self.topic.clone()).into())
            .await?;

        // Send our T topic to the remote peer.
        sink.send(TopicHandshakeMessage::Topic(self.topic.clone()))
            .await?;

        // Receive their Done message.
        let Some(message) = stream.next().await else {
            return Err(TopicHandshakeError::UnexpectedStreamClosure);
        };
        let message = message?;
        let TopicHandshakeMessage::Done = message else {
            return Err(TopicHandshakeError::UnexpectedMessage(message));
        };

        // Send our Done message.
        sink.send(TopicHandshakeMessage::Done).await?;

        // Announce that the topic handshake session has completed successfully.
        self.event_tx.send(TopicHandshakeEvent::Done.into()).await?;

        sink.flush().await?;
        self.event_tx.flush().await?;

        Ok(())
    }
}

/// Acceptor side of the topic handshake protocol.
///
/// After the protocol is complete both peers know the T of the initiator.
pub struct TopicHandshakeAcceptor<T, Evt> {
    pub event_tx: mpsc::Sender<Evt>,
    _phantom: PhantomData<T>,
}

impl<T, Evt> TopicHandshakeAcceptor<T, Evt>
where
    T: Clone + for<'de> Deserialize<'de> + Serialize,
    Evt: From<TopicHandshakeEvent<T>>,
{
    pub fn new(event_tx: mpsc::Sender<Evt>) -> Self {
        Self {
            event_tx,
            _phantom: PhantomData,
        }
    }
}

impl<T, Evt> Protocol for TopicHandshakeAcceptor<T, Evt>
where
    T: Clone + for<'de> Deserialize<'de> + Serialize,
    Evt: From<TopicHandshakeEvent<T>>,
{
    type Error = TopicHandshakeError<T>;
    type Output = T;
    type Message = TopicHandshakeMessage<T>;

    async fn run(
        mut self,
        sink: &mut (impl Sink<Self::Message, Error = Self::Error> + Unpin),
        stream: &mut (impl Stream<Item = Result<Self::Message, Self::Error>> + Unpin),
    ) -> Result<Self::Output, Self::Error> {
        // Announce that the topic handshake session has been accepted.
        self.event_tx
            .send(TopicHandshakeEvent::Accept.into())
            .await?;

        // Receive the remote peers T topic.
        let Some(message) = stream.next().await else {
            return Err(TopicHandshakeError::UnexpectedStreamClosure);
        };
        let message = message?;
        let TopicHandshakeMessage::Topic(topic) = message else {
            return Err(TopicHandshakeError::UnexpectedMessage(message));
        };

        // Announce that the topic was received.
        self.event_tx
            .send(TopicHandshakeEvent::TopicReceived(topic.clone()).into())
            .await?;

        // Send our Done message.
        sink.send(TopicHandshakeMessage::Done).await?;

        // Receive the remote peers Done message.
        let Some(message) = stream.next().await else {
            return Err(TopicHandshakeError::UnexpectedStreamClosure);
        };
        let message = message?;
        let TopicHandshakeMessage::Done = message else {
            return Err(TopicHandshakeError::UnexpectedMessage(message));
        };

        // Announce that the topic handshake session completed successfully.
        self.event_tx.send(TopicHandshakeEvent::Done.into()).await?;

        sink.flush().await?;
        self.event_tx.flush().await?;

        Ok(topic)
    }
}

/// Protocol message types.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub enum TopicHandshakeMessage<T> {
    Topic(T),
    Done,
}

/// Protocol error types.
#[derive(Debug, Error)]
pub enum TopicHandshakeError<T> {
    #[error("unexpected protocol message: {0}")]
    UnexpectedMessage(TopicHandshakeMessage<T>),

    #[error("stream ended before protocol completion")]
    UnexpectedStreamClosure,

    #[error("error sending on message sink: {0}")]
    MessageSink(String),

    #[error("error receiving from message stream: {0}")]
    MessageStream(String),

    #[error(transparent)]
    MpscSend(#[from] mpsc::SendError),
}

/// Events emitted from topic handshake protocol sessions.
#[derive(Debug, Clone, PartialEq)]
pub enum TopicHandshakeEvent<T> {
    Initiate(T),
    Accept,
    TopicReceived(T),
    Done,
}
