// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;

use ractor::ActorRef;
use thiserror::Error;
use tokio::sync::RwLock;

use crate::address_book::AddressBook;
use crate::iroh::Builder;
use crate::iroh::actors::ToIrohEndpoint;

#[derive(Clone)]
pub struct Endpoint {
    pub(crate) actor_ref: Arc<RwLock<ActorRef<ToIrohEndpoint>>>,
}

impl Endpoint {
    pub fn builder(address_book: AddressBook) -> Builder {
        Builder::new(address_book)
    }
}

#[derive(Debug, Error)]
pub enum EndpointError {
    /// Spawning the internal actor failed.
    #[error(transparent)]
    ActorSpawn(#[from] ractor::SpawnErr),

    /// Messaging with internal actor via RPC failed.
    #[error(transparent)]
    ActorRpc(#[from] ractor::RactorErr<ToIrohEndpoint>),
}
