// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;

use ractor::ActorRef;
use thiserror::Error;
use tokio::sync::RwLock;

use crate::address_book::AddressBook;
use crate::iroh_endpoint::Endpoint;
use crate::iroh_mdns::Builder;
use crate::iroh_mdns::actor::{MdnsActorArgs, ToMdns};

#[derive(Clone)]
pub struct MdnsDiscovery {
    pub(super) args: MdnsActorArgs,
    pub(super) inner: Arc<RwLock<Inner>>,
}

pub(super) struct Inner {
    pub(super) actor_ref: Option<ActorRef<ToMdns>>,
}

impl MdnsDiscovery {
    pub(crate) fn new(actor_ref: Option<ActorRef<ToMdns>>, args: MdnsActorArgs) -> Self {
        Self {
            args,
            inner: Arc::new(RwLock::new(Inner { actor_ref })),
        }
    }

    pub fn builder(address_book: AddressBook, endpoint: Endpoint) -> Builder {
        Builder::new(address_book, endpoint)
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        if let Some(actor_ref) = self.actor_ref.take() {
            actor_ref.stop(None);
        }
    }
}

#[derive(Debug, Error)]
pub enum MdnsDiscoveryError {
    /// Spawning the internal actor failed.
    #[error(transparent)]
    ActorSpawn(#[from] ractor::SpawnErr),

    /// Spawning the internal actor as a child actor of a supervisor failed.
    #[cfg(feature = "supervisor")]
    #[error(transparent)]
    ActorLinkedSpawn(#[from] crate::supervisor::SupervisorError),

    /// Messaging with internal actor via RPC failed.
    #[error(transparent)]
    ActorRpc(#[from] Box<ractor::RactorErr<ToMdns>>),
}
