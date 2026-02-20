// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_store_next::{SqliteStore, SqliteStoreBuilder};
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};

use crate::address_book::actor::AddressBookActor;
use crate::address_book::{AddressBook, AddressBookError};

pub struct Builder {
    pub(crate) store: Option<SqliteStore<'static>>,
}

impl Builder {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self { store: None }
    }

    pub fn store(mut self, store: SqliteStore<'static>) -> Self {
        self.store = Some(store);
        self
    }

    pub async fn spawn(self) -> Result<AddressBook, AddressBookError> {
        // Use in-memory address book store by default.
        let store = match self.store {
            Some(store) => store,
            None => SqliteStoreBuilder::new().build().await?,
        };

        let (actor_ref, _) = {
            let thread_pool = ThreadLocalActorSpawner::new();
            let args = (store,);
            AddressBookActor::spawn(None, args, thread_pool).await?
        };

        Ok(AddressBook::new(Some(actor_ref)))
    }
}
