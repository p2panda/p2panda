// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;

use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use tokio::sync::RwLock;

use crate::address_book::AddressBook;
use crate::iroh_endpoint::Endpoint;
use crate::iroh_mdns::actor::MdnsActor;
use crate::iroh_mdns::{MdnsDiscovery, MdnsDiscoveryError, MdnsDiscoveryMode};

pub struct Builder {
    mode: Option<MdnsDiscoveryMode>,
    endpoint: Endpoint,
    address_book: AddressBook,
}

impl Builder {
    pub fn new(address_book: AddressBook, endpoint: Endpoint) -> Self {
        Self {
            mode: None,
            endpoint,
            address_book,
        }
    }

    pub fn mode(mut self, mode: MdnsDiscoveryMode) -> Self {
        self.mode = Some(mode);
        self
    }

    pub async fn spawn(self) -> Result<MdnsDiscovery, MdnsDiscoveryError> {
        let (actor_ref, _) = {
            let thread_pool = ThreadLocalActorSpawner::new();

            let config = self.mode.unwrap_or_default();
            let args = (config, self.address_book, self.endpoint);

            MdnsActor::spawn(None, args, thread_pool).await?
        };

        Ok(MdnsDiscovery {
            actor_ref: Arc::new(RwLock::new(actor_ref)),
        })
    }
}
