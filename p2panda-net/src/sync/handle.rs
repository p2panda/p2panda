// SPDX-License-Identifier: MIT OR Apache-2.0

use futures_util::{Stream, StreamExt};
use p2panda_sync::FromSync;
use ractor::{ActorRef, call};
use thiserror::Error;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;

use crate::TopicId;
use crate::sync::actors::{ToSyncManager, ToTopicManager};

/// Handle to a sync stream.
///
/// The stream can be used to publish messages or to request a subscription.
pub struct SyncHandle<M, E>
where
    M: Clone + Send + 'static,
    E: Clone + Send + 'static,
{
    topic: TopicId,
    manager_ref: ActorRef<ToSyncManager<M, E>>,
    topic_manager_ref: ActorRef<ToTopicManager<M>>,
}

impl<M, E> SyncHandle<M, E>
where
    M: Clone + Send + 'static,
    E: Clone + Send + 'static,
{
    pub(crate) fn new(
        topic: TopicId,
        manager_ref: ActorRef<ToSyncManager<M, E>>,
        topic_manager_ref: ActorRef<ToTopicManager<M>>,
    ) -> Self {
        Self {
            topic,
            manager_ref,
            topic_manager_ref,
        }
    }

    /// Publishes a message to the stream.
    pub async fn publish(&self, data: M) -> Result<(), SyncHandleError<M, E>> {
        // This would likely be a critical failure for this stream handle, since we are unable to
        // send messages to the sync manager.
        self.topic_manager_ref
            .send_message(ToTopicManager::Publish(data))
            .map_err(Box::new)?;
        Ok(())
    }

    /// Subscribes to the stream.
    ///
    /// The returned `SyncSubscription` provides a means of receiving messages from
    /// the stream.
    pub async fn subscribe(&self) -> Result<SyncSubscription<E>, SyncHandleError<M, E>> {
        if let Some(stream) =
            call!(self.manager_ref, ToSyncManager::Subscribe, self.topic).map_err(Box::new)?
        {
            Ok(SyncSubscription::<E>::new(self.topic, stream))
        } else {
            Err(SyncHandleError::StreamNotFound)
        }
    }

    /// Returns the topic of the stream.
    pub fn topic(&self) -> TopicId {
        self.topic
    }

    /// Manually starts sync session with given node.
    ///
    /// If there's no transport information for this node this action will fail.
    // TODO: Consider making this public, for this we would need to decide if we want to receive
    // the sync session events and status directly as a stream from the return type?
    #[cfg(test)]
    pub fn initiate_session(&self, node_id: crate::NodeId) {
        self.manager_ref
            .send_message(ToSyncManager::InitiateSync(self.topic, node_id))
            .unwrap();
    }
}

impl<M, E> Drop for SyncHandle<M, E>
where
    M: Clone + Send + 'static,
    E: Clone + Send + 'static,
{
    fn drop(&mut self) {
        // Ignore error here as the actor might already be dropped.
        let _ = self
            .manager_ref
            .send_message(ToSyncManager::Close(self.topic));
    }
}

/// Handle to a sync subscription.
///
/// The stream can be used to receive messages from the stream.
pub struct SyncSubscription<E> {
    topic: TopicId,
    // Messages sent directly from the topic manager.
    from_sync_rx: BroadcastStream<FromSync<E>>,
}

impl<E> SyncSubscription<E>
where
    E: Clone + Send + 'static,
{
    pub(crate) fn new(topic: TopicId, from_sync_rx: broadcast::Receiver<FromSync<E>>) -> Self {
        Self {
            topic,
            from_sync_rx: BroadcastStream::new(from_sync_rx),
        }
    }

    /// Returns the topic of the stream.
    pub fn topic(&self) -> TopicId {
        self.topic
    }
}

impl<E> Stream for SyncSubscription<E>
where
    E: Clone + Send + 'static,
{
    type Item = Result<FromSync<E>, BroadcastStreamRecvError>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.from_sync_rx.poll_next_unpin(cx)
    }
}

#[derive(Debug, Error)]
pub enum SyncHandleError<M, E> {
    /// Messaging with internal actor via RPC failed.
    #[error(transparent)]
    ActorRpc(#[from] Box<ractor::RactorErr<ToSyncManager<M, E>>>),

    #[error(transparent)]
    Publish(#[from] Box<ractor::MessagingErr<ToTopicManager<M>>>),

    #[error("no stream exists for the given topic")]
    StreamNotFound,
}
