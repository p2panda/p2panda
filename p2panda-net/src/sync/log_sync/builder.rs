// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;
use std::marker::PhantomData;

use p2panda_core::{Extensions, Hash, LogId, Operation, PublicKey};
use p2panda_store::logs::LogStore;
use p2panda_store::topics::TopicStore;
use p2panda_sync::manager::TopicSyncManager;
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};

use crate::TopicId;
use crate::gossip::Gossip;
use crate::iroh_endpoint::Endpoint;
use crate::sync::actors::SyncManager;
use crate::sync::log_sync::{LOG_SYNC_PROTOCOL_ID, LogSync, LogSyncError};

pub struct Builder<S, L, E>
where
    S: LogStore<Operation<E>, PublicKey, L, u64, Hash>
        + TopicStore<TopicId, PublicKey, L>
        + Clone
        + Send
        + 'static,
    L: LogId + Debug + Send + 'static,
    E: Extensions + Send + 'static,
{
    store: S,
    endpoint: Endpoint,
    gossip: Gossip,
    _marker: PhantomData<(L, E)>,
}

impl<S, L, E> Builder<S, L, E>
where
    S: LogStore<Operation<E>, PublicKey, L, u64, Hash>
        + TopicStore<TopicId, PublicKey, L>
        + Clone
        + Send
        + 'static,
    L: LogId + Debug + Send + 'static,
    E: Extensions + Send + 'static,
{
    pub fn new(store: S, endpoint: Endpoint, gossip: Gossip) -> Self {
        Self {
            store,
            endpoint,
            gossip,
            _marker: PhantomData,
        }
    }

    pub async fn spawn(self) -> Result<LogSync<S, L, E>, LogSyncError<E>> {
        let (actor_ref, _) = {
            let thread_pool = ThreadLocalActorSpawner::new();

            let args = (
                LOG_SYNC_PROTOCOL_ID.to_vec(),
                self.store,
                self.endpoint,
                self.gossip,
            );

            SyncManager::<TopicSyncManager<TopicId, S, L, E>>::spawn(None, args, thread_pool)
                .await?
        };

        Ok(LogSync::new(actor_ref))
    }
}
