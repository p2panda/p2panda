// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;

use ractor::ActorRef;
use thiserror::Error;
use tokio::sync::RwLock;

use crate::NodeId;
use crate::address_book::AddressBook;
use crate::discovery::Builder;
use crate::discovery::actors::ToDiscoveryManager;
use crate::iroh::Endpoint;

#[derive(Clone, Debug)]
pub struct Discovery {
    pub(crate) actor_ref: Arc<RwLock<ActorRef<ToDiscoveryManager>>>,
}

impl Discovery {
    pub fn builder(my_node_id: NodeId, address_book: AddressBook, endpoint: Endpoint) -> Builder {
        Builder::new(my_node_id, address_book, endpoint)
    }
}

#[derive(Debug, Error)]
pub enum DiscoveryError {
    /// Spawning the internal actor failed.
    #[error(transparent)]
    ActorSpawn(#[from] ractor::SpawnErr),

    /// Messaging with internal actor via RPC failed.
    #[error(transparent)]
    ActorRpc(#[from] ractor::RactorErr<ToDiscoveryManager>),
}
