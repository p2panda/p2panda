// SPDX-License-Identifier: MIT OR Apache-2.0

use std::marker::PhantomData;

use p2panda_core::Extensions;
use p2panda_store::{LogId, LogStore, OperationStore};

use crate::address_book::AddressBook;
use crate::iroh_endpoint::Endpoint;
use crate::log_sync::{LogSync, LogSyncError};

pub struct Builder<S, L, E>
where
    S: OperationStore<L, E> + LogStore<L, E> + Send + 'static,
    L: LogId + Send + 'static,
    E: Extensions + Send + 'static,
{
    address_book: AddressBook,
    endpoint: Endpoint,
    store: S,
    _marker: PhantomData<(L, E)>,
}

impl<S, L, E> Builder<S, L, E>
where
    S: OperationStore<L, E> + LogStore<L, E> + Send + 'static,
    L: LogId + Send + 'static,
    E: Extensions + Send + 'static,
{
    pub fn new(store: S, address_book: AddressBook, endpoint: Endpoint) -> Self {
        Self {
            address_book,
            endpoint,
            store,
            _marker: PhantomData,
        }
    }

    pub async fn spawn(self) -> Result<LogSync<S, L, E>, LogSyncError<E>> {
        let (actor_ref, _) = {
            let thread_pool = ThreadLocalActorSpawner::new();
            let config = self.config.unwrap_or_default();
            let rng = self.rng.unwrap_or(ChaCha20Rng::from_os_rng());
            let args = (config, rng, self.address_book, self.endpoint);
            DiscoveryManager::spawn(None, args, thread_pool).await?
        };

        Ok(LogSync {
            actor_ref: Arc::new(RwLock::new(actor_ref)),
            address_book: self.address_book,
            endpoint: self.endpoint,
            store: self.store,
            _marker: PhantomData,
        })
    }
}
