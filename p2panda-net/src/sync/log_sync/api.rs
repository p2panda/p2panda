// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::Arc;

use p2panda_core::{Extensions, Operation};
use p2panda_store::{LogId, LogStore, OperationStore};
use p2panda_sync::protocols::{Logs, TopicLogSyncEvent};
use p2panda_sync::traits::TopicMap;
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

/// Eventually consistent, local-first sync protocol based on append-only logs.
///
/// ## Example
///
/// See [`chat.rs`] for a full example using the sync protocol.
///
/// ## Local-first
///
/// In local-first applications we want to converge towards the same state eventually, which
/// requires nodes to catch up on missed messages - independent of if they've been offline or
/// not.
///
/// `p2panda-net` comes with a default `LogSync` protocol implementation which uses p2panda's
/// **append-only log** Base Convergent Data Type (CDT).
///
/// After initial sync has finished, nodes switch to **live-mode** to directly push new messages to the
/// network using a gossip protocol.
///
/// [`chat.rs`]: https://github.com/p2panda/p2panda/blob/main/p2panda-net/examples/chat.rs
#[derive(Clone)]
pub struct LogSync<S, L, E, TM>
where
    S: OperationStore<L, E> + LogStore<L, E> + Send + 'static,
    L: LogId + Serialize + for<'de> Deserialize<'de> + Send + 'static,
    E: Extensions + Send + 'static,
    TM: TopicMap<TopicId, Logs<L>> + Send + 'static,
{
    inner: Arc<RwLock<Inner<E>>>,
    _phantom: PhantomData<(S, L, TM)>,
}

struct Inner<E>
where
    E: Extensions + Send + 'static,
{
    #[allow(clippy::type_complexity)]
    actor_ref: ActorRef<ToSyncManager<Operation<E>, TopicLogSyncEvent<E>>>,
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
        actor_ref: ActorRef<ToSyncManager<Operation<E>, TopicLogSyncEvent<E>>>,
    ) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner { actor_ref })),
            _phantom: PhantomData,
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
    ) -> Result<SyncHandle<Operation<E>, TopicLogSyncEvent<E>>, LogSyncError<E>> {
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

impl<E> Drop for Inner<E>
where
    E: Extensions + Send + 'static,
{
    fn drop(&mut self) {
        self.actor_ref.stop(None);
    }
}

#[derive(Debug, Error)]
pub enum LogSyncError<E> {
    /// Spawning the internal actor failed.
    #[error(transparent)]
    ActorSpawn(#[from] ractor::SpawnErr),

    /// Messaging with internal actor via RPC failed.
    #[error(transparent)]
    ActorRpc(#[from] Box<ractor::RactorErr<ToSyncManager<Operation<E>, TopicLogSyncEvent<E>>>>),
}
