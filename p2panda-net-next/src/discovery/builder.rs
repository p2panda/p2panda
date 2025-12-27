// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;

use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use tokio::sync::RwLock;

use crate::address_book::AddressBook;
use crate::discovery::actors::DiscoveryManager;
use crate::discovery::config::DiscoveryConfig;
use crate::discovery::{Discovery, DiscoveryError};
use crate::iroh_endpoint::Endpoint;

pub struct Builder {
    config: Option<DiscoveryConfig>,
    rng: Option<ChaCha20Rng>,
    address_book: AddressBook,
    endpoint: Endpoint,
}

impl Builder {
    pub fn new(address_book: AddressBook, endpoint: Endpoint) -> Self {
        Self {
            config: None,
            rng: None,
            address_book,
            endpoint,
        }
    }

    pub fn config(mut self, config: DiscoveryConfig) -> Self {
        self.config = Some(config);
        self
    }

    #[cfg(test)]
    pub fn rng(mut self, rng: ChaCha20Rng) -> Self {
        self.rng = Some(rng);
        self
    }

    pub async fn spawn(self) -> Result<Discovery, DiscoveryError> {
        let (actor_ref, _) = {
            let thread_pool = ThreadLocalActorSpawner::new();
            let config = self.config.unwrap_or_default();
            let rng = self.rng.unwrap_or(ChaCha20Rng::from_os_rng());
            let args = (config, rng, self.address_book, self.endpoint);
            DiscoveryManager::spawn(None, args, thread_pool).await?
        };

        Ok(Discovery {
            actor_ref: Arc::new(RwLock::new(actor_ref)),
        })
    }
}
