// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::{PrivateKey, PublicKey};
use ractor::thread_local::ThreadLocalActorSpawner;

use crate::NetworkId;
use crate::config::IrohConfig;

#[derive(Clone, Debug)]
pub struct ApplicationArguments {
    pub network_id: NetworkId,
    pub private_key: PrivateKey,
    pub public_key: PublicKey,
    pub iroh_config: IrohConfig,
    pub root_thread_pool: ThreadLocalActorSpawner,
}

pub struct ArgsBuilder {
    network_id: NetworkId,
    private_key: Option<PrivateKey>,
    iroh_config: Option<IrohConfig>,
}

impl ArgsBuilder {
    pub fn new(network_id: NetworkId) -> Self {
        Self {
            network_id,
            private_key: None,
            iroh_config: None,
        }
    }

    pub fn with_network_id(mut self, network_id: NetworkId) -> Self {
        self.network_id = network_id;
        self
    }

    pub fn with_iroh_config(mut self, config: IrohConfig) -> Self {
        self.iroh_config = Some(config);
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
            public_key: private_key.public_key(),
            private_key,
            iroh_config: self.iroh_config.unwrap_or_default(),
            root_thread_pool: ThreadLocalActorSpawner::new(),
        }
    }
}
