// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet};
use std::error::Error as StdError;
use std::marker::PhantomData;

// @TODO: This will come from `p2panda-store` eventually.
use p2panda_discovery::address_book::AddressBookStore;
use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef, Message};

use crate::args::ApplicationArguments;
use crate::{NodeId, NodeInfo, TopicId};

/// Address book actor name.
pub const ADDRESS_BOOK: &str = "net.address_book";

pub enum ToAddressBook {}

pub struct AddressBookState<S> {
    store: S,
}

pub struct AddressBook<S, T> {
    _marker: PhantomData<(S, T)>,
}

impl<S, T> Default for AddressBook<S, T> {
    fn default() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<S, T> ThreadLocalActor for AddressBook<S, T>
where
    S: AddressBookStore<T, NodeId, NodeInfo> + Send + 'static,
    S::Error: StdError + Send + Sync + 'static,
    T: 'static,
{
    type State = AddressBookState<S>;

    type Msg = ToAddressBook;

    // @TODO: For now we leave out the concept of a `NetworkId` but we may want some way to slice
    // address subsets in the future.
    type Arguments = ApplicationArguments<S>;

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(AddressBookState { store: args.store })
    }
}
