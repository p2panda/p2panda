// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::Arc;

use p2panda_core::Extensions;
use p2panda_store::{LogId, LogStore, OperationStore};
use p2panda_sync::TopicSyncManager;
use p2panda_sync::managers::topic_sync_manager::TopicSyncManagerConfig;
use p2panda_sync::topic_log_sync::TopicLogMap;
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::TopicId;
use crate::address_book::AddressBook;
use crate::gossip::Gossip;
use crate::iroh_endpoint::Endpoint;
use crate::log_sync::actors::LogSyncStream;
use crate::log_sync::{LogSync, LogSyncError};

pub struct Builder<S, L, E, TM>
where
    S: Debug + OperationStore<L, E> + LogStore<L, E> + Send + Sync + 'static,
    L: LogId + Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static,
    E: Extensions + Send + Sync + 'static,
    TM: Clone + Debug + TopicLogMap<TopicId, L> + Send + Sync + 'static,
{
    store: S,
    topic_map: TM,
    address_book: AddressBook,
    endpoint: Endpoint,
    gossip: Gossip,
    _marker: PhantomData<(L, E)>,
}

impl<S, L, E, TM> Builder<S, L, E, TM>
where
    S: Debug + OperationStore<L, E> + LogStore<L, E> + Send + Sync + 'static,
    L: LogId + Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static,
    E: Extensions + Send + Sync + 'static,
    TM: Clone + Debug + TopicLogMap<TopicId, L> + Send + Sync + 'static,
{
    pub fn new(
        store: S,
        topic_map: TM,
        address_book: AddressBook,
        endpoint: Endpoint,
        gossip: Gossip,
    ) -> Self {
        Self {
            store,
            topic_map,
            address_book,
            endpoint,
            gossip,
            _marker: PhantomData,
        }
    }

    pub async fn spawn(
        self,
    ) -> Result<LogSync<S, L, E, TM>, LogSyncError<TopicSyncManager<TopicId, S, TM, L, E>>> {
        let (actor_ref, _) = {
            let thread_pool = ThreadLocalActorSpawner::new();

            let config = TopicSyncManagerConfig {
                store: self.store,
                topic_map: self.topic_map,
            };
            let args = (config, self.address_book, self.endpoint, self.gossip);

            LogSyncStream::<TopicSyncManager<TopicId, S, TM, L, E>>::spawn(None, args, thread_pool)
                .await?
        };

        Ok(LogSync {
            actor_ref: Arc::new(RwLock::new(actor_ref)),
        })
    }
}
