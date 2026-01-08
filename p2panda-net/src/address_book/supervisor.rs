// SPDX-License-Identifier: MIT OR Apache-2.0

use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{ActorCell, SpawnErr};

use crate::address_book::actor::AddressBookActor;
use crate::address_book::{AddressBook, AddressBookError, Builder};
use crate::supervisor::{ChildActor, ChildActorFut, Supervisor};

impl Builder {
    pub async fn spawn_linked(
        self,
        supervisor: &Supervisor,
    ) -> Result<AddressBook, AddressBookError> {
        let address_book = AddressBook::new(None);
        supervisor.start_child_actor(address_book.clone()).await?;
        Ok(address_book)
    }
}

impl ChildActor for AddressBook {
    fn on_start(
        &self,
        supervisor: ActorCell,
        thread_pool: ThreadLocalActorSpawner,
    ) -> ChildActorFut<'_> {
        Box::pin(async move {
            // Spawn our actor as a child of the supervisor.
            let (actor_ref, _) = AddressBookActor::spawn_linked(
                None,
                (self
                    .store()
                    .await
                    .map_err(|err| SpawnErr::StartupFailed(err.into()))?,),
                supervisor,
                thread_pool,
            )
            .await?;

            // Update the reference to our actor, we need this to send messages to it.
            let mut inner = self.inner.write().await;
            inner.actor_ref.replace(actor_ref.clone());

            Ok(actor_ref.into())
        })
    }
}
