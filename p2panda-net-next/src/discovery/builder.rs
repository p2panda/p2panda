// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;

use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use tokio::sync::RwLock;

use crate::NodeId;
use crate::address_book::AddressBook;
use crate::config::DiscoveryConfig;
use crate::discovery::actors::DiscoveryManager;
use crate::discovery::{Discovery, DiscoveryError};
use crate::iroh::Endpoint;

pub struct Builder {
    my_node_id: NodeId,
    config: Option<DiscoveryConfig>,
    rng: Option<ChaCha20Rng>,
    address_book: AddressBook,
    endpoint: Endpoint,
}

impl Builder {
    pub fn new(my_node_id: NodeId, address_book: AddressBook, endpoint: Endpoint) -> Self {
        Self {
            my_node_id,
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
            let args = (
                self.my_node_id,
                config,
                rng,
                self.address_book,
                self.endpoint,
            );
            DiscoveryManager::spawn(None, args, thread_pool).await?
        };

        Ok(Discovery {
            actor_ref: Arc::new(RwLock::new(actor_ref)),
        })
    }
}
