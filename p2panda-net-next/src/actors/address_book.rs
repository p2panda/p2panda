// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet};

use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef, Message};

use crate::{NodeId, NodeInfo, TopicId};

/// Address book actor name.
pub const ADDRESS_BOOK: &str = "net.address_book";

pub enum ToAddressBook {}

pub struct AddressBookState {}

#[derive(Default)]
pub struct AddressBook;

impl ThreadLocalActor for AddressBook {
    type State = AddressBookState;

    type Msg = ToAddressBook;

    // @TODO: For now we leave out the concept of a `NetworkId` but we may want some way to slice
    // address subsets in the future.
    type Arguments = ();

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        _args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(AddressBookState {})
    }
}
