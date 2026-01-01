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
    #[allow(unused)]
    inner: Arc<RwLock<Inner>>,
}

#[derive(Clone)]
struct Inner {
    actor_ref: ActorRef<ToMdns>,
}

impl MdnsDiscovery {
    pub(crate) fn new(actor_ref: ActorRef<ToMdns>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner { actor_ref })),
        }
    }

    pub fn builder(address_book: AddressBook, endpoint: Endpoint) -> Builder {
        Builder::new(address_book, endpoint)
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.actor_ref.stop(None);
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
