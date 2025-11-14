// SPDX-License-Identifier: MIT OR Apache-2.0

//! Eventually consistent stream types and associated methods.
//!
//! Eventually consistent streams provide an interface for publishing messages into the network and
//! receiving messages from the network. They are intended to be used for catching up on past state
//! and then optionally receiving the latest updates for the given topic.
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
use crate::actors::sync::{SyncManager, ToSyncManager};
use crate::actors::{ActorNamespace, with_namespace};
use crate::network::{FromNetwork, ToNetwork};
use crate::streams::StreamError;

/// A handle to an eventually consistent messaging stream.
///
/// The stream can be used to publish messages or to request a subscription.
pub struct EventuallyConsistentStream {
    actor_namespace: ActorNamespace,
    topic_id: TopicId,
    sync_manager: ActorRef<ToSyncManager>,
}

impl EventuallyConsistentStream {
    /// Returns a handle to an eventually consistent messaging stream.
    pub(crate) fn new(
        actor_namespace: ActorNamespace,
        topic_id: TopicId,
        sync_manager: ActorRef<ToSyncManager>,
    ) -> Self {
        Self {
            actor_namespace,
            topic_id,
            sync_manager,
        }
    }

    /// Publishes a message to the stream.
    pub async fn publish(&self, bytes: impl Into<Vec<u8>>) -> Result<(), StreamError<Vec<u8>>> {
        // TODO: Error handling; we need an appropriate variant on `StreamError`.
        //
        // This would likely be a critical failure for this stream handle, since we are unable to
        // send messages to the sync manager.
        self.sync_manager
            .send_message(ToSyncManager::Publish(self.topic_id, bytes))?;

        Ok(())
    }

    /// Subscribes to the stream.
    ///
    /// The returned `EventuallyConsistentSubscription` provides a means of receiving messages from
    /// the stream.
    pub async fn subscribe(&self) -> Result<EventuallyConsistentSubscription, StreamError<()>> {
        // Get a reference to the eventually consistent streams actor.
        let actor = self
            .eventually_consistent_streams_actor()
            .ok_or(StreamError::Subscribe(self.topic_id))?;

        // Ask the eventually consistent streams actor for a subscription.
        if let Some(stream) = call!(
            actor,
            ToEventuallyConsistentStreams::Subscribe,
            self.topic_id
        )
        .map_err(|_| StreamError::Subscribe(self.topic_id))?
        {
            Ok(stream)
        } else {
            Err(StreamError::StreamNotFound)
        }
    }

    /// Returns the topic ID of the stream.
    pub fn topic_id(&self) -> TopicId {
        self.topic_id
    }

    /// Closes the eventually consistent messaging stream.
    pub fn close(self) -> Result<(), StreamError<()>> {
        // Get a reference to the ephemeral streams actor.
        let actor = self
            .eventually_consistent_streams_actor()
            .ok_or(StreamError::Close(self.topic_id))?;

        actor
            .cast(ToEventuallyConsistentStreams::Close(self.topic_id))
            .map_err(|_| StreamError::Close(self.topic_id))?;

        Ok(())
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
    // Messages sent directly from the sync manager.
    from_sync_rx: BroadcastReceiver<FromNetwork>,
}

// TODO: Implement `Stream` for `BroadcastReceiver`.

impl EventuallyConsistentSubscription {
    /// Returns a handle to an eventually consistent messaging stream subscriber.
    pub(crate) fn new(topic_id: TopicId, from_sync_rx: BroadcastReceiver<FromNetwork>) -> Self {
        Self {
            topic_id,
            from_sync_rx,
        }
    }

    /// Receives the next message from the stream.
    pub async fn recv(&mut self) -> Result<FromNetwork, StreamError<()>> {
        self.from_sync_rx.recv().await.map_err(StreamError::Recv)
    }

    /// Attempts to return a pending value on this receiver without awaiting.
    pub fn try_recv(&mut self) -> Result<FromNetwork, StreamError<()>> {
        self.from_sync_rx.try_recv().map_err(StreamError::TryRecv)
    }

    /// Returns the topic ID of the stream.
    pub fn topic_id(&self) -> TopicId {
        self.topic_id
    }
}
