// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;

use ractor::{ActorRef, call};
use thiserror::Error;
use tokio::sync::{RwLock, broadcast};

use crate::address_book::AddressBook;
use crate::discovery::actors::ToDiscoveryManager;
use crate::discovery::events::DiscoveryEvent;
use crate::discovery::{Builder, DiscoveryMetrics};
use crate::iroh_endpoint::Endpoint;

#[derive(Clone)]
pub struct Discovery {
    inner: Arc<RwLock<Inner>>,
}

#[derive(Clone)]
struct Inner {
    actor_ref: ActorRef<ToDiscoveryManager>,
}

impl Discovery {
    pub(crate) fn new(actor_ref: ActorRef<ToDiscoveryManager>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner { actor_ref })),
        }
    }

    pub fn builder(address_book: AddressBook, endpoint: Endpoint) -> Builder {
        Builder::new(address_book, endpoint)
    }

    /// Subscribe to system events.
    pub async fn events(&self) -> Result<broadcast::Receiver<DiscoveryEvent>, DiscoveryError> {
        let inner = self.inner.read().await;
        let result = call!(inner.actor_ref, ToDiscoveryManager::Events).map_err(Box::new)?;
        Ok(result)
    }

    /// Returns current metrics.
    pub async fn metrics(&self) -> Result<DiscoveryMetrics, DiscoveryError> {
        let inner = self.inner.read().await;
        let result = call!(inner.actor_ref, ToDiscoveryManager::Metrics).map_err(Box::new)?;
        Ok(result)
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.actor_ref.stop(None);
    }
}

#[derive(Debug, Error)]
pub enum DiscoveryError {
    /// Spawning the internal actor failed.
    #[error(transparent)]
    ActorSpawn(#[from] ractor::SpawnErr),

    /// Messaging with internal actor via RPC failed.
    #[error(transparent)]
    ActorRpc(#[from] Box<ractor::RactorErr<ToDiscoveryManager>>),
}
