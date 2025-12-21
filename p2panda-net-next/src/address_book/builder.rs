// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error as StdError;
use std::marker::PhantomData;
use std::sync::Arc;

use p2panda_discovery::address_book::AddressBookStore;
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use tokio::sync::RwLock;

use crate::address_book::actor::AddressBookActor;
use crate::address_book::{AddressBook, AddressBookError};
use crate::addrs::{NodeId, NodeInfo};

pub struct Builder<S> {
    pub(crate) my_id: NodeId,
    pub(crate) store: S,
}

impl<S> Builder<S>
where
    S: AddressBookStore<NodeId, NodeInfo> + Send + 'static,
    S::Error: StdError + Send + Sync + 'static,
{
    pub async fn spawn(self) -> Result<AddressBook<S>, AddressBookError<S>> {
        let (actor_ref, _) = {
            let thread_pool = ThreadLocalActorSpawner::new();
            let args = (self.my_id, self.store);
            AddressBookActor::<S>::spawn(None, args, thread_pool).await?
        };

        Ok(AddressBook {
            actor_ref: Arc::new(RwLock::new(actor_ref)),
            _marker: PhantomData,
        })
    }
}
