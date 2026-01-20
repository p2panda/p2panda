// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;

use ractor::{ActorRef, call};
use thiserror::Error;
use tokio::sync::{RwLock, broadcast};

use crate::address_book::AddressBook;
use crate::discovery::actors::{DiscoveryManagerArgs, ToDiscoveryManager};
use crate::discovery::events::DiscoveryEvent;
use crate::discovery::{Builder, DiscoveryMetrics};
use crate::iroh_endpoint::Endpoint;

/// Confidential topic discovery and random-walk strategy to resolve transport infos.
///
/// ## Design
///
/// Read more about the underlying design in [`p2panda-discovery`].
///
/// [`p2panda-discovery`]: https://docs.rs/p2panda-discovery/latest/p2panda_discovery/
#[derive(Clone)]
pub struct Discovery {
    #[allow(unused, reason = "used by supervisor behind feature flag")]
    pub(super) args: DiscoveryManagerArgs,
    pub(super) inner: Arc<RwLock<Inner>>,
}

pub(super) struct Inner {
    pub(super) actor_ref: Option<ActorRef<ToDiscoveryManager>>,
}

impl Discovery {
    pub(crate) fn new(
        actor_ref: Option<ActorRef<ToDiscoveryManager>>,
        args: DiscoveryManagerArgs,
    ) -> Self {
        Self {
            args,
            inner: Arc::new(RwLock::new(Inner { actor_ref })),
        }
    }

    pub fn builder(address_book: AddressBook, endpoint: Endpoint) -> Builder {
        Builder::new(address_book, endpoint)
    }

    /// Subscribe to system events.
    pub async fn events(&self) -> Result<broadcast::Receiver<DiscoveryEvent>, DiscoveryError> {
        let inner = self.inner.read().await;
        let result = call!(
            inner.actor_ref.as_ref().expect("actor spawned in builder"),
            ToDiscoveryManager::Events
        )
        .map_err(Box::new)?;
        Ok(result)
    }

    /// Returns current metrics.
    pub async fn metrics(&self) -> Result<DiscoveryMetrics, DiscoveryError> {
        let inner = self.inner.read().await;
        let result = call!(
            inner.actor_ref.as_ref().expect("actor spawned in builder"),
            ToDiscoveryManager::Metrics
        )
        .map_err(Box::new)?;
        Ok(result)
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
pub enum DiscoveryError {
    /// Spawning the internal actor failed.
    #[error(transparent)]
    ActorSpawn(#[from] ractor::SpawnErr),

    /// Spawning the internal actor as a child actor of a supervisor failed.
    #[cfg(feature = "supervisor")]
    #[error(transparent)]
    ActorLinkedSpawn(#[from] crate::supervisor::SupervisorError),

    /// Messaging with internal actor via RPC failed.
    #[error(transparent)]
    ActorRpc(#[from] Box<ractor::RactorErr<ToDiscoveryManager>>),
}
