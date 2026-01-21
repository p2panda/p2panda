// SPDX-License-Identifier: MIT OR Apache-2.0

use std::marker::PhantomData;

use p2panda_core::Extensions;
use p2panda_store::{LogId, LogStore, OperationStore};
use p2panda_sync::manager::{TopicSyncManager, TopicSyncManagerArgs};
use p2panda_sync::protocols::Logs;
use p2panda_sync::traits::TopicMap;
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use serde::{Deserialize, Serialize};

use crate::TopicId;
use crate::gossip::Gossip;
use crate::iroh_endpoint::Endpoint;
use crate::sync::actors::SyncManager;
use crate::sync::log_sync::{LOG_SYNC_PROTOCOL_ID, LogSync, LogSyncError};

pub struct Builder<S, L, E, TM>
where
    S: OperationStore<L, E> + LogStore<L, E> + Send + 'static,
    L: LogId + Serialize + for<'de> Deserialize<'de> + Send + 'static,
    E: Extensions + Send + 'static,
    TM: TopicMap<TopicId, Logs<L>> + Send + 'static,
{
    store: S,
    topic_map: TM,
    endpoint: Endpoint,
    gossip: Gossip,
    _marker: PhantomData<(L, E)>,
}

impl<S, L, E, TM> Builder<S, L, E, TM>
where
    S: OperationStore<L, E> + LogStore<L, E> + Send + 'static,
    L: LogId + Serialize + for<'de> Deserialize<'de> + Send + 'static,
    E: Extensions + Send + 'static,
    TM: TopicMap<TopicId, Logs<L>> + Send + 'static,
{
    pub fn new(store: S, topic_map: TM, endpoint: Endpoint, gossip: Gossip) -> Self {
        Self {
            store,
            topic_map,
            endpoint,
            gossip,
            _marker: PhantomData,
        }
    }

    pub async fn spawn(self) -> Result<LogSync<S, L, E, TM>, LogSyncError<E>> {
        let (actor_ref, _) = {
            let thread_pool = ThreadLocalActorSpawner::new();

            let config = TopicSyncManagerArgs {
                store: self.store,
                topic_map: self.topic_map,
            };

            let args = (
                LOG_SYNC_PROTOCOL_ID.to_vec(),
                config,
                self.endpoint,
                self.gossip,
            );

            SyncManager::<TopicSyncManager<TopicId, S, TM, L, E>>::spawn(None, args, thread_pool)
                .await?
        };

        Ok(LogSync::new(actor_ref))
    }
}
