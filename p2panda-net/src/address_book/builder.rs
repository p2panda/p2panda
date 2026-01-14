// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error as StdError;

use p2panda_discovery::address_book::{AddressBookStore, BoxedAddressBookStore};
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

use crate::NodeId;
use crate::address_book::actor::AddressBookActor;
use crate::address_book::{AddressBook, AddressBookError};
use crate::addrs::NodeInfo;

pub struct Builder {
    pub(crate) store: Option<BoxedAddressBookStore<NodeId, NodeInfo>>,
}

impl Builder {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self { store: None }
    }

    pub fn store<S>(mut self, store: S) -> Self
    where
        S: Clone + AddressBookStore<NodeId, NodeInfo> + Send + 'static,
        S::Error: StdError + Send + Sync + 'static,
    {
        self.store = Some(Box::new(store));
        self
    }

    pub async fn spawn(self) -> Result<AddressBook, AddressBookError> {
        // Use in-memory address book store by default.
        let store = self.store.unwrap_or_else(|| {
            let rng = ChaCha20Rng::from_os_rng();
            let store = p2panda_discovery::address_book::memory::MemoryStore::new(rng);
            Box::new(store)
        });

        let (actor_ref, _) = {
            let thread_pool = ThreadLocalActorSpawner::new();
            let args = (store,);
            AddressBookActor::spawn(None, args, thread_pool).await?
        };

        Ok(AddressBook::new(Some(actor_ref)))
    }
}
