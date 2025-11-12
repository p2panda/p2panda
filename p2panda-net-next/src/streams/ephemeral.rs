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
use ractor::{ActorRef, call, registry};
use thiserror::Error;
use tokio::sync::broadcast::Receiver as BroadcastReceiver;
use tokio::sync::broadcast::error::{RecvError, TryRecvError};
use tokio::sync::mpsc::Sender;
use tokio::sync::mpsc::error::SendError;

use crate::TopicId;
use crate::actors::streams::ephemeral::{EPHEMERAL_STREAMS, ToEphemeralStreams};
use crate::actors::{ActorNamespace, with_namespace};
use crate::network::{FromNetwork, ToNetwork};

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
        // Get a reference to the ephemeral streams actor.
        if let Some(actor) = self.ephemeral_streams_actor() {
            // Ask the ephemeral streams actor for a subscription.
            if let Some(stream) = call!(actor, ToEphemeralStreams::Subscribe, self.topic_id)
                .map_err(|_| StreamError::Actor(EPHEMERAL_STREAMS.to_string()))?
            {
                Ok(stream)
            } else {
                Err(StreamError::StreamNotFound)
            }
        } else {
            Err(StreamError::ActorNotFound(EPHEMERAL_STREAMS.to_string()))
        }
    }

    /// Returns the topic ID of the stream.
    pub fn topic_id(&self) -> TopicId {
        self.topic_id
    }

    /// Closes the ephemeral messaging stream.
    pub fn close(self) -> Result<(), StreamError<()>> {
        if let Some(actor) = self.ephemeral_streams_actor() {
            actor
                .cast(ToEphemeralStreams::Unsubscribe(self.topic_id))
                .map_err(|_| StreamError::Actor(EPHEMERAL_STREAMS.to_string()))?;

            Ok(())
        } else {
            Err(StreamError::ActorNotFound(EPHEMERAL_STREAMS.to_string()))
        }
    }

    /// Internal helper to get a reference to the ephemeral streams actor.
    fn ephemeral_streams_actor(&self) -> Option<ActorRef<ToEphemeralStreams>> {
        if let Some(ephemeral_streams_actor) =
            registry::where_is(with_namespace(EPHEMERAL_STREAMS, &self.actor_namespace))
        {
            let actor: ActorRef<ToEphemeralStreams> = ephemeral_streams_actor.into();

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
