// SPDX-License-Identifier: MIT OR Apache-2.0

use std::marker::PhantomData;
use std::sync::Arc;

use p2panda_core::Extensions;
use p2panda_store::{LogId, LogStore, OperationStore};
use ractor::ActorRef;
use thiserror::Error;
use tokio::sync::RwLock;

use crate::address_book::AddressBook;
use crate::iroh_endpoint::Endpoint;
use crate::log_sync::Builder;
use crate::log_sync::actors::ToSyncManager;

#[derive(Clone)]
pub struct LogSync<S, L, E>
where
    S: OperationStore<L, E> + LogStore<L, E> + Send + 'static,
    L: LogId + Send + 'static,
    // TODO: Extensions should be generic over a stream handle, not over this struct.
    E: Extensions + Send + 'static,
{
    pub(crate) address_book: AddressBook,
    pub(crate) endpoint: Endpoint,
    pub(crate) store: S,
    pub(crate) _marker: PhantomData<(L, E)>,
}

impl<S, L, E> LogSync<S, L, E>
where
    S: OperationStore<L, E> + LogStore<L, E> + Send + 'static,
    L: LogId + Send + 'static,
    E: Extensions + Send + 'static,
{
    pub fn builder(store: S, address_book: AddressBook, endpoint: Endpoint) -> Builder<S, L, E> {
        Builder::<S, L, E>::new(store, address_book, endpoint)
    }
}

#[derive(Debug, Error)]
pub enum LogSyncError<E>
where
    E: Extensions,
{
    /// Spawning the internal actor failed.
    #[error(transparent)]
    ActorSpawn(#[from] ractor::SpawnErr),

    /// Messaging with internal actor via RPC failed.
    #[error(transparent)]
    ActorRpc(#[from] ractor::RactorErr<ToSyncManager<E>>),
}
