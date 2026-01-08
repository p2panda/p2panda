// SPDX-License-Identifier: MIT OR Apache-2.0

use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};

use crate::address_book::AddressBook;
use crate::iroh_endpoint::Endpoint;
use crate::iroh_mdns::actor::{MdnsActor, MdnsActorArgs};
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

    pub(crate) fn build_args(self) -> MdnsActorArgs {
        let config = self.mode.unwrap_or_default();
        (config, self.address_book, self.endpoint)
    }

    pub async fn spawn(self) -> Result<MdnsDiscovery, MdnsDiscoveryError> {
        let args = self.build_args();

        let (actor_ref, _) = {
            let thread_pool = ThreadLocalActorSpawner::new();
            MdnsActor::spawn(None, args.clone(), thread_pool).await?
        };

        Ok(MdnsDiscovery::new(Some(actor_ref), args))
    }
}
