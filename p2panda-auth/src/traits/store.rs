// SPDX-License-Identifier: MIT OR Apache-2.0

// TODO: Rename this module from `group_store.rs` to `store.rs`.

use std::error::Error;
use std::fmt::Display;

use crate::group::{GroupControlMessage, GroupCrdtState};
use crate::traits::{IdentityHandle, OperationId, Orderer};

/// Interface for interacting with a global group store.
pub trait GroupStore<ID, OP, C, RS, ORD>
where
    ID: IdentityHandle,
    OP: OperationId,
    ORD: Orderer<ID, OP, GroupControlMessage<ID, C>>,
    Self: Sized,
{
    type Error: Error + Display;

    /// Insert a group state into the store.
    fn insert(
        &self,
        id: &ID,
        group: &GroupCrdtState<ID, OP, C, RS, ORD, Self>,
    ) -> Result<(), Self::Error>;

    /// Get a group's state from the store.
    #[allow(clippy::type_complexity)]
    fn get(&self, id: &ID)
    -> Result<Option<GroupCrdtState<ID, OP, C, RS, ORD, Self>>, Self::Error>;
}
