// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;

use ractor::{ActorRef, call};
use thiserror::Error;
use tokio::sync::{RwLock, broadcast};

use crate::address_book::AddressBook;
use crate::discovery::actors::ToDiscoveryManager;
use crate::discovery::events::DiscoveryEvent;
use crate::discovery::{Builder, DiscoveryMetrics};
use crate::iroh::Endpoint;

#[derive(Clone, Debug)]
pub struct Discovery {
    pub(crate) actor_ref: Arc<RwLock<ActorRef<ToDiscoveryManager>>>,
}

impl Discovery {
    pub fn builder(address_book: AddressBook, endpoint: Endpoint) -> Builder {
        Builder::new(address_book, endpoint)
    }

    /// Subscribe to system events.
    pub async fn events(&self) -> Result<broadcast::Receiver<DiscoveryEvent>, DiscoveryError> {
        let result = call!(self.actor_ref.read().await, ToDiscoveryManager::Events)?;
        Ok(result)
    }

    /// Returns current metrics.
    pub async fn metrics(&self) -> Result<DiscoveryMetrics, DiscoveryError> {
        let result = call!(self.actor_ref.read().await, ToDiscoveryManager::Metrics)?;
        Ok(result)
    }
}

#[derive(Debug, Error)]
pub enum DiscoveryError {
    /// Spawning the internal actor failed.
    #[error(transparent)]
    ActorSpawn(#[from] ractor::SpawnErr),

    // TODO: The error type gets very large due to including the ToDiscoveryManager manager, we
    // should convert it to types _not_ containing the message itself.
    /// Messaging with internal actor via RPC failed.
    #[error(transparent)]
    ActorRpc(#[from] ractor::RactorErr<ToDiscoveryManager>),
}
