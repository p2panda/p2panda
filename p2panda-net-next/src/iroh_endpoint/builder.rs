// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;

use p2panda_core::PrivateKey;
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use tokio::sync::RwLock;

use crate::NetworkId;
use crate::address_book::AddressBook;
use crate::iroh_endpoint::actors::IrohEndpoint;
use crate::iroh_endpoint::api::{Endpoint, EndpointError};
use crate::iroh_endpoint::config::IrohConfig;

pub const DEFAULT_NETWORK_ID: NetworkId = [
    247, 69, 248, 242, 132, 120, 159, 230, 98, 100, 214, 200, 78, 40, 79, 94, 174, 8, 12, 27, 84,
    195, 246, 159, 132, 240, 79, 208, 1, 43, 132, 118,
];

pub struct Builder {
    network_id: Option<NetworkId>,
    private_key: Option<PrivateKey>,
    config: Option<IrohConfig>,
    address_book: AddressBook,
}

impl Builder {
    pub fn new(address_book: AddressBook) -> Self {
        Self {
            network_id: None,
            private_key: None,
            config: None,
            address_book,
        }
    }

    pub fn network_id(mut self, network_id: NetworkId) -> Self {
        self.network_id = Some(network_id);
        self
    }

    pub fn private_key(mut self, private_key: PrivateKey) -> Self {
        self.private_key = Some(private_key);
        self
    }

    pub fn config(mut self, config: IrohConfig) -> Self {
        self.config = Some(config);
        self
    }

    pub async fn spawn(self) -> Result<Endpoint, EndpointError> {
        let network_id = self.network_id.unwrap_or(DEFAULT_NETWORK_ID);
        let private_key = self.private_key.unwrap_or_default();
        let my_node_id = private_key.public_key();

        let (actor_ref, _) = {
            let thread_pool = ThreadLocalActorSpawner::new();

            let config = self.config.unwrap_or_default();
            let args = (network_id, private_key, config, self.address_book);

            IrohEndpoint::spawn(None, args, thread_pool).await?
        };

        Ok(Endpoint {
            network_id,
            my_node_id,
            actor_ref: Arc::new(RwLock::new(actor_ref)),
        })
    }
}
