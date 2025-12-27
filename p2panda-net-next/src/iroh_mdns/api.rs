// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;

use ractor::ActorRef;
use thiserror::Error;
use tokio::sync::RwLock;

use crate::address_book::AddressBook;
use crate::iroh_endpoint::Endpoint;
use crate::iroh_mdns::Builder;
use crate::iroh_mdns::actor::ToMdns;

#[derive(Clone)]
pub struct MdnsDiscovery {
    pub(crate) actor_ref: Arc<RwLock<ActorRef<ToMdns>>>,
}

impl MdnsDiscovery {
    pub fn builder(address_book: AddressBook, endpoint: Endpoint) -> Builder {
        Builder::new(address_book, endpoint)
    }
}

#[derive(Debug, Error)]
pub enum MdnsDiscoveryError {
    /// Spawning the internal actor failed.
    #[error(transparent)]
    ActorSpawn(#[from] ractor::SpawnErr),

    /// Messaging with internal actor via RPC failed.
    #[error(transparent)]
    ActorRpc(#[from] ractor::RactorErr<ToMdns>),
}
