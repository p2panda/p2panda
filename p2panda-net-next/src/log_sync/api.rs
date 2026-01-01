// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;
use std::sync::Arc;

use p2panda_core::Extensions;
use p2panda_store::{LogId, LogStore, OperationStore};
use p2panda_sync::topic_log_sync::TopicLogMap;
use p2panda_sync::traits::SyncManager;
use p2panda_sync::{FromSync, TopicSyncManager};
use ractor::{ActorRef, call};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{RwLock, broadcast};

use crate::TopicId;
use crate::address_book::AddressBook;
use crate::gossip::Gossip;
use crate::iroh_endpoint::Endpoint;
use crate::log_sync::Builder;
use crate::log_sync::actors::{ToLogSyncStream, ToSyncManager};

#[derive(Clone)]
pub struct LogSync<S, L, E, TM>
where
    // TODO: Extensions should be generic over a stream handle, not over this struct.
    S: Debug + OperationStore<L, E> + LogStore<L, E> + Send + Sync + 'static,
    L: LogId + Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static,
    E: Extensions + Send + Sync + 'static,
    TM: Clone + Debug + TopicLogMap<TopicId, L> + Send + Sync + 'static,
{
    pub(crate) actor_ref:
        Arc<RwLock<ActorRef<ToLogSyncStream<TopicSyncManager<TopicId, S, TM, L, E>>>>>,
}

impl<S, L, E, TM> LogSync<S, L, E, TM>
where
    S: Debug + OperationStore<L, E> + LogStore<L, E> + Send + Sync + 'static,
    L: LogId + Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static,
    E: Extensions + Send + Sync + 'static,
    TM: Clone + Debug + TopicLogMap<TopicId, L> + Send + Sync + 'static,
{
    pub fn builder(
        store: S,
        topic_map: TM,
        address_book: AddressBook,
        endpoint: Endpoint,
        gossip: Gossip,
    ) -> Builder<S, L, E, TM> {
        Builder::<S, L, E, TM>::new(store, topic_map, address_book, endpoint, gossip)
    }

    pub async fn stream(
        &self,
        topic: TopicId,
        live_mode: bool,
    ) -> Result<
        EventuallyConsistentStream<TopicSyncManager<TopicId, S, TM, L, E>>,
        LogSyncError<TopicSyncManager<TopicId, S, TM, L, E>>,
    > {
        let actor_ref = self.actor_ref.read().await;
        let sync_manager_ref = call!(actor_ref, ToLogSyncStream::Create, topic, live_mode)?;
        Ok(EventuallyConsistentStream::new(
            topic,
            actor_ref.clone(),
            sync_manager_ref,
        ))
    }
}

#[derive(Debug, Error)]
pub enum LogSyncError<M>
where
    M: SyncManager<TopicId> + Send + 'static,
{
    /// Spawning the internal actor failed.
    #[error(transparent)]
    ActorSpawn(#[from] ractor::SpawnErr),

    /// Messaging with internal actor via RPC failed.
    #[error(transparent)]
    ActorRpc(#[from] ractor::RactorErr<ToLogSyncStream<M>>),
}

/// A handle to an eventually consistent messaging stream.
///
/// The stream can be used to publish messages or to request a subscription.
pub struct EventuallyConsistentStream<M>
where
    M: SyncManager<TopicId> + Send + 'static,
{
    topic: TopicId,
    stream_ref: ActorRef<ToLogSyncStream<M>>,
    manager_ref: ActorRef<ToSyncManager<M::Message>>,
}

impl<M> EventuallyConsistentStream<M>
where
    M: SyncManager<TopicId> + Send + 'static,
{
    pub(crate) fn new(
        topic: TopicId,
        stream_ref: ActorRef<ToLogSyncStream<M>>,
        manager_ref: ActorRef<ToSyncManager<M::Message>>,
    ) -> Self {
        Self {
            topic,
            stream_ref,
            manager_ref,
        }
    }

    /// Publishes a message to the stream.
    pub async fn publish(&self, message: M::Message) -> Result<(), StreamError<M::Message>> {
        // This would likely be a critical failure for this stream handle, since we are unable to
        // send messages to the sync manager.
        self.manager_ref
            .send_message(ToSyncManager::Publish {
                topic: self.topic,
                data: message,
            })
            // TODO: change this when we decide on error propagation strategy.
            .map_err(|_| StreamError::Publish(self.topic))?;

        Ok(())
    }

    /// Subscribes to the stream.
    ///
    /// The returned `EventuallyConsistentSubscription` provides a means of receiving messages from
    /// the stream.
    pub async fn subscribe(
        &self,
    ) -> Result<EventuallyConsistentSubscription<M::Event>, StreamError<()>> {
        if let Some(stream) = call!(self.stream_ref, ToLogSyncStream::Subscribe, self.topic)
            .map_err(|_| StreamError::Subscribe(self.topic))?
        {
            Ok(EventuallyConsistentSubscription::new(self.topic, stream))
        } else {
            Err(StreamError::StreamNotFound)
        }
    }

    /// Returns the topic of the stream.
    pub fn topic(&self) -> TopicId {
        self.topic
    }
}

/// A handle to an eventually consistent messaging stream subscription.
///
/// The stream can be used to receive messages from the stream.
pub struct EventuallyConsistentSubscription<Ev> {
    topic: TopicId,
    // Messages sent directly from the sync manager.
    from_sync_rx: broadcast::Receiver<FromSync<Ev>>,
}

// TODO: Implement `Stream`.

impl<Ev> EventuallyConsistentSubscription<Ev>
where
    Ev: Clone + Send + 'static,
{
    pub(crate) fn new(topic: TopicId, from_sync_rx: broadcast::Receiver<FromSync<Ev>>) -> Self {
        Self {
            topic,
            from_sync_rx,
        }
    }

    /// Receives the next message from the stream.
    pub async fn recv(&mut self) -> Result<FromSync<Ev>, StreamError<()>> {
        self.from_sync_rx.recv().await.map_err(StreamError::Recv)
    }

    /// Attempts to return a pending value on this receiver without awaiting.
    pub fn try_recv(&mut self) -> Result<FromSync<Ev>, StreamError<()>> {
        self.from_sync_rx.try_recv().map_err(StreamError::TryRecv)
    }

    /// Returns the topic of the stream.
    pub fn topic(&self) -> TopicId {
        self.topic
    }
}

#[derive(Debug, Error)]
pub enum StreamError<T> {
    #[error(transparent)]
    Send(#[from] broadcast::error::SendError<T>),

    #[error(transparent)]
    Recv(#[from] broadcast::error::RecvError),

    #[error(transparent)]
    TryRecv(#[from] broadcast::error::TryRecvError),

    #[error("failed to create stream for topic {0:?} due to system error")]
    Create(TopicId),

    #[error("failed to subscribe to topic {0:?} due to system error")]
    Subscribe(TopicId),

    #[error("failed to close stream for topic {0:?}")]
    Close(TopicId),

    #[error("no stream exists for the given topic")]
    StreamNotFound,

    #[error("failed to publish to topic {0:?} due to system error")]
    Publish(TopicId),
}
