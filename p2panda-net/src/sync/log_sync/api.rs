// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;
use std::sync::Arc;

use p2panda_core::Extensions;
use p2panda_store::{LogId, LogStore, OperationStore};
use p2panda_sync::manager::TopicSyncManager;
use p2panda_sync::protocols::Logs;
use p2panda_sync::traits::{Manager as SyncManagerTrait, TopicMap};
use ractor::{ActorRef, call};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::RwLock;

use crate::TopicId;
use crate::gossip::Gossip;
use crate::iroh_endpoint::Endpoint;
use crate::sync::actors::ToSyncManager;
use crate::sync::handle::SyncHandle;
use crate::sync::log_sync::Builder;

#[derive(Clone)]
pub struct LogSync<S, L, E, TM>
where
    S: OperationStore<L, E> + LogStore<L, E> + Send + 'static,
    L: LogId + Serialize + for<'de> Deserialize<'de> + Send + 'static,
    E: Extensions + Send + 'static,
    TM: TopicMap<TopicId, Logs<L>> + Send + 'static,
{
    inner: Arc<RwLock<Inner<S, L, E, TM>>>,
}

struct Inner<S, L, E, TM>
where
    S: OperationStore<L, E> + LogStore<L, E> + Send + 'static,
    L: LogId + Serialize + for<'de> Deserialize<'de> + Send + 'static,
    E: Extensions + Send + 'static,
    TM: TopicMap<TopicId, Logs<L>> + Send + 'static,
{
    #[allow(clippy::type_complexity)]
    actor_ref: ActorRef<ToSyncManager<TopicSyncManager<TopicId, S, TM, L, E>>>,
}

impl<S, L, E, TM> LogSync<S, L, E, TM>
where
    S: OperationStore<L, E> + LogStore<L, E> + Send + 'static,
    L: LogId + Serialize + for<'de> Deserialize<'de> + Send + 'static,
    E: Extensions + Send + 'static,
    TM: TopicMap<TopicId, Logs<L>> + Send + 'static,
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
        endpoint: Endpoint,
        gossip: Gossip,
    ) -> Builder<S, L, E, TM> {
        Builder::<S, L, E, TM>::new(store, topic_map, endpoint, gossip)
    }

    // TODO: Extensions should be generic over a stream handle, not over this struct.
    pub async fn stream(
        &self,
        topic: TopicId,
        live_mode: bool,
    ) -> Result<
        SyncHandle<TopicSyncManager<TopicId, S, TM, L, E>>,
        LogSyncError<TopicSyncManager<TopicId, S, TM, L, E>>,
    > {
        let inner = self.inner.read().await;
        let sync_manager_ref =
            call!(inner.actor_ref, ToSyncManager::Create, topic, live_mode).map_err(Box::new)?;

        Ok(SyncHandle::new(
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
    TM: TopicMap<TopicId, Logs<L>> + Send + 'static,
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
