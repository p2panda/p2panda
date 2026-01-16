// SPDX-License-Identifier: MIT OR Apache-2.0

use futures_util::{Stream, StreamExt};
use p2panda_sync::FromSync;
use p2panda_sync::traits::Manager as SyncManagerTrait;
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
pub struct SyncHandle<M>
where
    M: SyncManagerTrait<TopicId> + Send + 'static,
{
    topic: TopicId,
    manager_ref: ActorRef<ToSyncManager<M>>,
    topic_manager_ref: ActorRef<ToTopicManager<M::Message>>,
}

impl<M> SyncHandle<M>
where
    M: SyncManagerTrait<TopicId> + Send + 'static,
{
    pub(crate) fn new(
        topic: TopicId,
        manager_ref: ActorRef<ToSyncManager<M>>,
        topic_manager_ref: ActorRef<ToTopicManager<M::Message>>,
    ) -> Self {
        Self {
            topic,
            manager_ref,
            topic_manager_ref,
        }
    }

    /// Publishes a message to the stream.
    pub async fn publish(&self, data: M::Message) -> Result<(), SyncHandleError<M>> {
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
    pub async fn subscribe(&self) -> Result<SyncSubscription<M>, SyncHandleError<M>> {
        if let Some(stream) =
            call!(self.manager_ref, ToSyncManager::Subscribe, self.topic).map_err(Box::new)?
        {
            Ok(SyncSubscription::<M>::new(self.topic, stream))
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

impl<M> Drop for SyncHandle<M>
where
    M: SyncManagerTrait<TopicId> + Send + 'static,
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
pub struct SyncSubscription<M>
where
    M: SyncManagerTrait<TopicId> + Send + 'static,
{
    topic: TopicId,
    // Messages sent directly from the topic manager.
    from_sync_rx: BroadcastStream<FromSync<M::Event>>,
}

impl<M> SyncSubscription<M>
where
    M: SyncManagerTrait<TopicId> + Send + 'static,
{
    pub(crate) fn new(
        topic: TopicId,
        from_sync_rx: broadcast::Receiver<FromSync<M::Event>>,
    ) -> Self {
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

impl<M> Stream for SyncSubscription<M>
where
    M: SyncManagerTrait<TopicId> + Send + 'static,
{
    type Item = Result<FromSync<M::Event>, SyncHandleError<M>>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.from_sync_rx
            .poll_next_unpin(cx)
            .map_err(SyncHandleError::from)
    }
}

#[derive(Debug, Error)]
pub enum SyncHandleError<M>
where
    M: SyncManagerTrait<TopicId> + Send + 'static,
{
    /// Messaging with internal actor via RPC failed.
    #[error(transparent)]
    ActorRpc(#[from] Box<ractor::RactorErr<ToSyncManager<M>>>),

    #[error(transparent)]
    Publish(#[from] Box<ractor::MessagingErr<ToTopicManager<M::Message>>>),

    #[error(transparent)]
    Subscribe(#[from] BroadcastStreamRecvError),

    #[error("no stream exists for the given topic")]
    StreamNotFound,
}
