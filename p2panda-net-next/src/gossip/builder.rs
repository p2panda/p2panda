// SPDX-License-Identifier: MIT OR Apache-2.0

use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};

use crate::address_book::AddressBook;
use crate::gossip::actors::GossipManager;
use crate::gossip::api::{Gossip, GossipError};
use crate::iroh::Endpoint;

pub struct Builder {
    address_book: AddressBook,
    endpoint: Endpoint,
}

impl Builder {
    pub fn new(address_book: AddressBook, endpoint: Endpoint) -> Self {
        Self {
            address_book,
            endpoint,
        }
    }

    pub async fn spawn(self) -> Result<Gossip, GossipError> {
        let my_node_id = self.endpoint.node_id();

        let (actor_ref, _) = {
            let thread_pool = ThreadLocalActorSpawner::new();
            let args = (self.address_book.clone(), self.endpoint);
            GossipManager::spawn(None, args, thread_pool).await?
        };

        Ok(Gossip::new(actor_ref, my_node_id, self.address_book))
    }
}
