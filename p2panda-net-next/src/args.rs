// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::{PrivateKey, PublicKey};
use ractor::thread_local::ThreadLocalActorSpawner;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

use crate::NetworkId;
use crate::config::{DiscoveryConfig, IrohConfig};

#[derive(Clone, Debug)]
pub struct ApplicationArguments {
    pub network_id: NetworkId,
    pub rng: ChaCha20Rng,
    pub private_key: PrivateKey,
    pub public_key: PublicKey,
    pub iroh_config: IrohConfig,
    pub discovery_config: DiscoveryConfig,
    pub root_thread_pool: ThreadLocalActorSpawner,
}

pub struct ArgsBuilder {
    network_id: NetworkId,
    rng: Option<ChaCha20Rng>,
    private_key: Option<PrivateKey>,
    iroh_config: Option<IrohConfig>,
    discovery_config: Option<DiscoveryConfig>,
}

#[allow(unused)]
impl ArgsBuilder {
    pub fn new(network_id: NetworkId) -> Self {
        Self {
            network_id,
            rng: None,
            private_key: None,
            iroh_config: None,
            discovery_config: None,
        }
    }

    pub fn with_network_id(mut self, network_id: NetworkId) -> Self {
        self.network_id = network_id;
        self
    }

    pub fn with_rng(mut self, rng: ChaCha20Rng) -> Self {
        self.rng = Some(rng);
        self
    }

    pub fn with_iroh_config(mut self, config: IrohConfig) -> Self {
        self.iroh_config = Some(config);
        self
    }

    pub fn with_discovery_config(mut self, config: DiscoveryConfig) -> Self {
        self.discovery_config = Some(config);
        self
    }

    pub fn with_private_key(mut self, private_key: PrivateKey) -> Self {
        self.private_key = Some(private_key);
        self
    }

    pub fn build(self) -> ApplicationArguments {
        let private_key = self.private_key.unwrap_or_default();
        ApplicationArguments {
            network_id: self.network_id,
            rng: self.rng.unwrap_or(ChaCha20Rng::from_os_rng()),
            public_key: private_key.public_key(),
            private_key,
            iroh_config: self.iroh_config.unwrap_or_default(),
            discovery_config: self.discovery_config.unwrap_or_default(),
            root_thread_pool: ThreadLocalActorSpawner::new(),
        }
    }
}
