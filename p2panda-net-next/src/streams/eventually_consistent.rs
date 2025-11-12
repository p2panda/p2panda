// SPDX-License-Identifier: MIT OR Apache-2.0

// TODO: Correct logic; this is simply a copy-paste of the ephemeral stream types for the moment.

//! Eventually consistent stream types and associated methods.
//!
//! Eventually consistent streams provide an interface for publishing messages into the network and
//! receiving messages from the network.
//!
//! Ephemeral streams are intended to be used for relatively short-lived messages without
//! persistence and catch-up of past state. In most cases, messages will only be received if they
//! were published after the subscription was created. The exception to this is if the message was
//! still propagating through the network at the time of the subscription; then it's possible that
//! the message is received, even though the publication time was strictly before that of the local
//! subscription event.
//!
//! Use the ephemeral stream if you simply want to send and receive messages without first
//! synchronising past state with others nodes.
use ractor::{ActorRef, call, registry};
use thiserror::Error;
use tokio::sync::broadcast::Receiver as BroadcastReceiver;
use tokio::sync::broadcast::error::{RecvError, TryRecvError};
use tokio::sync::mpsc::Sender;
use tokio::sync::mpsc::error::SendError;

use crate::TopicId;
use crate::actors::streams::eventually_consistent::{
    EVENTUALLY_CONSISTENT_STREAMS, ToEventuallyConsistentStreams,
};
use crate::actors::{ActorNamespace, with_namespace};
use crate::network::{FromNetwork, ToNetwork};
use crate::streams::StreamError;

/// A handle to an eventually consistent messaging stream.
///
/// The stream can be used to publish messages or to request a subscription.
pub struct EventuallyConsistentStream {
    topic_id: TopicId,
    to_topic_tx: Sender<ToNetwork>,
    actor_namespace: ActorNamespace,
}

impl EventuallyConsistentStream {
    /// Returns a handle to an eventually consistent messaging stream.
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
    /// The returned `EventuallyConsistentSubscription` provides a means of receiving messages from
    /// the stream.
    pub async fn subscribe(&self) -> Result<EventuallyConsistentSubscription, StreamError<()>> {
        // Get a reference to the eventually consistent streams actor.
        if let Some(actor) = self.eventually_consistent_streams_actor() {
            // Ask the eventually consistent streams actor for a subscription.
            if let Some(stream) = call!(
                actor,
                ToEventuallyConsistentStreams::Subscribe,
                self.topic_id
            )
            .map_err(|_| StreamError::Actor(EVENTUALLY_CONSISTENT_STREAMS.to_string()))?
            {
                Ok(stream)
            } else {
                Err(StreamError::StreamNotFound)
            }
        } else {
            Err(StreamError::ActorNotFound(
                EVENTUALLY_CONSISTENT_STREAMS.to_string(),
            ))
        }
    }

    /// Returns the topic ID of the stream.
    pub fn topic_id(&self) -> TopicId {
        self.topic_id
    }

    /// Closes the eventually consistent messaging stream.
    pub fn close(self) -> Result<(), StreamError<()>> {
        if let Some(actor) = self.eventually_consistent_streams_actor() {
            actor
                .cast(ToEventuallyConsistentStreams::Unsubscribe(self.topic_id))
                .map_err(|_| StreamError::Actor(EVENTUALLY_CONSISTENT_STREAMS.to_string()))?;

            Ok(())
        } else {
            Err(StreamError::ActorNotFound(
                EVENTUALLY_CONSISTENT_STREAMS.to_string(),
            ))
        }
    }

    /// Internal helper to get a reference to the eventually consistent streams actor.
    fn eventually_consistent_streams_actor(
        &self,
    ) -> Option<ActorRef<ToEventuallyConsistentStreams>> {
        if let Some(eventually_consistent_streams_actor) = registry::where_is(with_namespace(
            EVENTUALLY_CONSISTENT_STREAMS,
            &self.actor_namespace,
        )) {
            let actor: ActorRef<ToEventuallyConsistentStreams> =
                eventually_consistent_streams_actor.into();

            Some(actor)
        } else {
            None
        }
    }
}

/// A handle to an eventually consistent messaging stream subscription.
///
/// The stream can be used to receive messages from the stream.
pub struct EventuallyConsistentSubscription {
    topic_id: TopicId,
    from_topic_rx: BroadcastReceiver<FromNetwork>,
}

// TODO: Implement `Stream` for `BroadcastReceiver`.

impl EventuallyConsistentSubscription {
    /// Returns a handle to an eventually consistent messaging stream subscriber.
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
