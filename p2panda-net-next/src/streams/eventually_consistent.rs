// SPDX-License-Identifier: MIT OR Apache-2.0

//! Topic stream types and associated methods.
//!
//! Topic streams provide an interface for publishing messages into the network and receiving
//! messages from the network.
//!
//! Ephemeral streams are intended to be used for relatively short-lived messages without
//! persistence and catch-up of past state. In most cases, messages will only be received if they
//! were published after the subscription was created. The exception to this is if the message was
//! still propagating through the network at the time of the subscription; then it's possible that
//! the message is received, even though the publication time was strictly before that of the local
//! subscription event.
//!
//! Use the standard topic stream if you wish to receive past state and (optionally) messages
//! representing the latest updates in an ongoing manner.
use ractor::{call, registry, ActorRef};
use thiserror::Error;
use tokio::sync::broadcast::error::{RecvError, TryRecvError};
use tokio::sync::broadcast::Receiver as BroadcastReceiver;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::Sender;

use crate::actors::stream::{ToStream, STREAM};
use crate::actors::{with_namespace, ActorNamespace};
use crate::network::{FromNetwork, ToNetwork};
use crate::TopicId;

#[derive(Debug, Error)]
pub enum StreamError<T> {
    #[error(transparent)]
    Send(#[from] SendError<T>),

    #[error(transparent)]
    Recv(#[from] RecvError),

    #[error("actor {0} failed to process request")]
    Actor(String),

    #[error("failed to call {0} actor; it may be in the process of restarting")]
    ActorNotFound(String),

    #[error("no stream exists for the given topic")]
    StreamNotFound,
}

/// A handle to an ephemeral messaging stream.
///
/// The stream can be used to publish messages or to request a subscription.
pub struct EphemeralStream {
    topic_id: TopicId,
    to_topic_tx: Sender<ToNetwork>,
    actor_namespace: ActorNamespace,
}

impl EphemeralStream {
    /// Returns a handle to an ephemeral messaging stream.
    pub(crate) fn new(
        topic_id: TopicId,
        to_topic_tx: Sender<ToNetwork>,
        actor_namespace: ActorNamespace,
    ) -> Self {
        Self {
            topic_id,
            to_topic_tx,
            actor_namespace,
        }
    }

    /// Publishes a message to the stream.
    pub async fn publish(&self, bytes: impl Into<Vec<u8>>) -> Result<(), StreamError<Vec<u8>>> {
        self.to_topic_tx.send(bytes.into()).await?;

        Ok(())
    }

    /// Subscribes to the stream.
    ///
    /// The returned `EphemeralSubscription` provides a means of receiving messages from the
    /// stream.
    pub async fn subscribe(&self) -> Result<EphemeralSubscription, StreamError<()>> {
        // Get a reference to the stream actor.
        if let Some(actor) = self.stream_actor() {
            // Ask the stream actor for an ephemeral stream subscriber.
            if let Some(stream) = call!(actor, ToStream::EphemeralSubscription, self.topic_id)
                .map_err(|_| StreamError::Actor(STREAM.to_string()))?
            {
                Ok(stream)
            } else {
                Err(StreamError::StreamNotFound)
            }
        } else {
            Err(StreamError::ActorNotFound(STREAM.to_string()))
        }
    }

    /// Returns the topic ID of the stream.
    pub fn topic_id(&self) -> TopicId {
        self.topic_id
    }

    /// Closes the ephemeral messaging stream.
    pub fn close(self) -> Result<(), StreamError<()>> {
        if let Some(actor) = self.stream_actor() {
            actor
                .cast(ToStream::UnsubscribeEphemeral(self.topic_id))
                .map_err(|_| StreamError::Actor(STREAM.to_string()))?;

            Ok(())
        } else {
            Err(StreamError::ActorNotFound(STREAM.to_string()))
        }
    }

    /// Internal helper to get a reference to the stream actor.
    fn stream_actor(&self) -> Option<ActorRef<ToStream>> {
        if let Some(stream_actor) =
            registry::where_is(with_namespace(STREAM, &self.actor_namespace))
        {
            let actor: ActorRef<ToStream> = stream_actor.into();

            Some(actor)
        } else {
            None
        }
    }
}

/// A handle to an ephemeral messaging stream subscription.
///
/// The stream can be used to receive messages from the stream.
pub struct EphemeralSubscription {
    topic_id: TopicId,
    from_topic_rx: BroadcastReceiver<FromNetwork>,
}

// TODO: Implement `Stream` for `BroadcastReceiver`.

impl EphemeralSubscription {
    /// Returns a handle to an ephemeral messaging stream subscriber.
    pub(crate) fn new(topic_id: TopicId, from_topic_rx: BroadcastReceiver<FromNetwork>) -> Self {
        Self {
            topic_id,
            from_topic_rx,
        }
    }

    /// Receives the next message from the stream.
    pub async fn recv(&mut self) -> Result<FromNetwork, StreamError<()>> {
        self.from_topic_rx.recv().await.map_err(StreamError::Recv)
    }

    /// Attempts to return a pending value on this receiver without awaiting.
    pub fn try_recv(&mut self) -> Result<FromNetwork, TryRecvError> {
        self.from_topic_rx.try_recv()
    }

    /// Returns the topic ID of the stream.
    pub fn topic_id(&self) -> TopicId {
        self.topic_id
    }
}

/// A handle to an eventually-consistent messaging stream.
///
/// The stream can be used to publish messages or to request a subscription.
pub struct Stream {
    topic_id: TopicId,
    to_topic_tx: Sender<ToNetwork>,
    actor_namespace: ActorNamespace,
}

impl Stream {
    /// Returns a handle to an eventually-consistent messaging stream.
    pub(crate) fn new(
        topic_id: TopicId,
        to_topic_tx: Sender<ToNetwork>,
        actor_namespace: ActorNamespace,
    ) -> Self {
        Self {
            topic_id,
            to_topic_tx,
            actor_namespace,
        }
    }

    /// Publishes a message to the stream.
    pub async fn publish(&self, bytes: impl Into<Vec<u8>>) -> Result<(), StreamError<Vec<u8>>> {
        self.to_topic_tx.send(bytes.into()).await?;

        Ok(())
    }

    /// Subscribes to the stream.
    ///
    /// The returned `StreamSubscription` provides a means of receiving messages from the stream.
    pub async fn subscribe(&self) -> Result<StreamSubscription, StreamError<()>> {
        // Get a reference to the stream actor.
        if let Some(actor) = self.stream_actor() {
            // Ask the stream actor for a stream subscriber.
            if let Some(stream) = call!(actor, ToStream::Subscription, self.topic_id)
                .map_err(|_| StreamError::Actor(STREAM.to_string()))?
            {
                Ok(stream)
            } else {
                Err(StreamError::StreamNotFound)
            }
        } else {
            Err(StreamError::ActorNotFound(STREAM.to_string()))
        }
    }

    /// Returns the topic ID of the stream.
    pub fn topic_id(&self) -> TopicId {
        self.topic_id
    }

    /// Closes the messaging stream.
    pub fn close(self) -> Result<(), StreamError<()>> {
        if let Some(actor) = self.stream_actor() {
            actor
                .cast(ToStream::Unsubscribe(self.topic_id))
                .map_err(|_| StreamError::Actor(STREAM.to_string()))?;

            Ok(())
        } else {
            Err(StreamError::ActorNotFound(STREAM.to_string()))
        }
    }

    /// Internal helper to get a reference to the stream actor.
    fn stream_actor(&self) -> Option<ActorRef<ToStream>> {
        if let Some(stream_actor) =
            registry::where_is(with_namespace(STREAM, &self.actor_namespace))
        {
            let actor: ActorRef<ToStream> = stream_actor.into();

            Some(actor)
        } else {
            None
        }
    }
}

/// A handle to an eventually-consistent messaging stream subscription.
///
/// The stream can be used to receive messages from the stream.
pub struct StreamSubscription {
    topic_id: TopicId,
    from_topic_rx: BroadcastReceiver<FromNetwork>,
}

impl StreamSubscription {
    /// Returns a handle to an eventually-consistent messaging stream subscriber.
    pub(crate) fn new(topic_id: TopicId, from_topic_rx: BroadcastReceiver<FromNetwork>) -> Self {
        Self {
            topic_id,
            from_topic_rx,
        }
    }

    /// Receives the next message from the stream.
    pub async fn recv(&mut self) -> Result<FromNetwork, StreamError<()>> {
        self.from_topic_rx.recv().await.map_err(StreamError::Recv)
    }

    /// Attempts to return a pending value on this receiver without awaiting.
    pub fn try_recv(&mut self) -> Result<FromNetwork, TryRecvError> {
        self.from_topic_rx.try_recv()
    }

    /// Returns the topic ID of the stream.
    pub fn topic_id(&self) -> TopicId {
        self.topic_id
    }
}
