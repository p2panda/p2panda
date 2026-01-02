// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;
use std::sync::Arc;

use futures_util::{Stream, StreamExt};
use p2panda_core::Extensions;
use p2panda_store::{LogId, LogStore, OperationStore};
use p2panda_sync::topic_log_sync::TopicLogMap;
use p2panda_sync::traits::SyncManager as SyncManagerTrait;
use p2panda_sync::{FromSync, TopicSyncManager};
use ractor::{ActorRef, call};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{RwLock, broadcast};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;

use crate::TopicId;
use crate::address_book::AddressBook;
use crate::gossip::Gossip;
use crate::iroh_endpoint::Endpoint;
use crate::log_sync::Builder;
use crate::log_sync::actors::{ToSyncManager, ToTopicManager};

#[derive(Clone)]
pub struct LogSync<S, L, E, TM>
where
    S: OperationStore<L, E> + LogStore<L, E> + Send + 'static,
    L: LogId + Serialize + for<'de> Deserialize<'de> + Send + 'static,
    E: Extensions + Send + 'static,
    TM: TopicLogMap<TopicId, L> + Send + 'static,
{
    inner: Arc<RwLock<Inner<S, L, E, TM>>>,
}

#[derive(Clone)]
pub struct Inner<S, L, E, TM>
where
    S: OperationStore<L, E> + LogStore<L, E> + Send + 'static,
    L: LogId + Serialize + for<'de> Deserialize<'de> + Send + 'static,
    E: Extensions + Send + 'static,
    TM: TopicLogMap<TopicId, L> + Send + 'static,
{
    #[allow(clippy::type_complexity)]
    actor_ref: ActorRef<ToSyncManager<TopicSyncManager<TopicId, S, TM, L, E>>>,
}

impl<S, L, E, TM> LogSync<S, L, E, TM>
where
    S: OperationStore<L, E> + LogStore<L, E> + Send + 'static,
    L: LogId + Serialize + for<'de> Deserialize<'de> + Send + 'static,
    E: Extensions + Send + 'static,
    TM: TopicLogMap<TopicId, L> + Send + 'static,
{
    #[allow(clippy::type_complexity)]
    pub(crate) fn new(
        actor_ref: ActorRef<ToSyncManager<TopicSyncManager<TopicId, S, TM, L, E>>>,
    ) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner { actor_ref })),
        }
    }

    pub fn builder(
        store: S,
        topic_map: TM,
        address_book: AddressBook,
        endpoint: Endpoint,
        gossip: Gossip,
    ) -> Builder<S, L, E, TM> {
        Builder::<S, L, E, TM>::new(store, topic_map, address_book, endpoint, gossip)
    }

    // TODO: Extensions should be generic over a stream handle, not over this struct.
    pub async fn stream(
        &self,
        topic: TopicId,
        live_mode: bool,
    ) -> Result<
        LogSyncHandle<TopicSyncManager<TopicId, S, TM, L, E>>,
        LogSyncError<TopicSyncManager<TopicId, S, TM, L, E>>,
    > {
        let inner = self.inner.read().await;
        let sync_manager_ref =
            call!(inner.actor_ref, ToSyncManager::Create, topic, live_mode).map_err(Box::new)?;
        Ok(LogSyncHandle::new(
            topic,
            inner.actor_ref.clone(),
            sync_manager_ref,
        ))
    }
}

impl<S, L, E, TM> Drop for Inner<S, L, E, TM>
where
    S: OperationStore<L, E> + LogStore<L, E> + Send + 'static,
    L: LogId + Serialize + for<'de> Deserialize<'de> + Send + 'static,
    E: Extensions + Send + 'static,
    TM: TopicLogMap<TopicId, L> + Send + 'static,
{
    fn drop(&mut self) {
        self.actor_ref.stop(None);
    }
}

#[derive(Debug, Error)]
pub enum LogSyncError<M>
where
    M: SyncManagerTrait<TopicId> + Send + 'static,
{
    /// Spawning the internal actor failed.
    #[error(transparent)]
    ActorSpawn(#[from] ractor::SpawnErr),

    /// Messaging with internal actor via RPC failed.
    #[error(transparent)]
    ActorRpc(#[from] Box<ractor::RactorErr<ToSyncManager<M>>>),
}

/// A handle to an eventually consistent messaging stream.
///
/// The stream can be used to publish messages or to request a subscription.
pub struct LogSyncHandle<M>
where
    M: SyncManagerTrait<TopicId> + Send + 'static,
{
    topic: TopicId,
    manager_ref: ActorRef<ToSyncManager<M>>,
    topic_manager_ref: ActorRef<ToTopicManager<M::Message>>,
}

impl<M> LogSyncHandle<M>
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
    pub async fn publish(&self, data: M::Message) -> Result<(), LogSyncHandleError<M>> {
        // This would likely be a critical failure for this stream handle, since we are unable to
        // send messages to the sync manager.
        self.topic_manager_ref
            .send_message(ToTopicManager::Publish {
                topic: self.topic,
                data,
            })
            .map_err(Box::new)?;
        Ok(())
    }

    /// Subscribes to the stream.
    ///
    /// The returned `LogSyncSubscription` provides a means of receiving messages from
    /// the stream.
    pub async fn subscribe(&self) -> Result<LogSyncSubscription<M>, LogSyncHandleError<M>> {
        if let Some(stream) =
            call!(self.manager_ref, ToSyncManager::Subscribe, self.topic).map_err(Box::new)?
        {
            Ok(LogSyncSubscription::<M>::new(self.topic, stream))
        } else {
            Err(LogSyncHandleError::StreamNotFound)
        }
    }

    /// Returns the topic of the stream.
    pub fn topic(&self) -> TopicId {
        self.topic
    }

    /// Manually starts sync session with given node.
    ///
    /// If there's no transport information for this node this action will fail.
    // TODO: Consider making this a public method.
    #[cfg(test)]
    pub(crate) async fn initiate_session(&self, node_id: crate::NodeId) {
        self.manager_ref
            .send_message(ToSyncManager::InitiateSync(self.topic, node_id))
            .unwrap();
    }
}

impl<M> Drop for LogSyncHandle<M>
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

/// A handle to an eventually consistent messaging stream subscription.
///
/// The stream can be used to receive messages from the stream.
pub struct LogSyncSubscription<M>
where
    M: SyncManagerTrait<TopicId> + Send + 'static,
{
    topic: TopicId,
    // Messages sent directly from the topic manager.
    from_sync_rx: BroadcastStream<FromSync<M::Event>>,
}

impl<M> LogSyncSubscription<M>
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

impl<M> Stream for LogSyncSubscription<M>
where
    M: SyncManagerTrait<TopicId> + Send + 'static,
{
    type Item = Result<FromSync<M::Event>, LogSyncHandleError<M>>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.from_sync_rx
            .poll_next_unpin(cx)
            .map_err(LogSyncHandleError::from)
    }
}

#[derive(Debug, Error)]
pub enum LogSyncHandleError<M>
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
