// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error;

use crate::group::{GroupControlMessage, GroupState};
use crate::traits::{IdentityHandle, OperationId, Ordering};

/// API for global group store.
pub trait GroupStore<ID, OP, C, RS, ORD>
where
    ID: IdentityHandle,
    OP: OperationId,
    ORD: Ordering<ID, OP, GroupControlMessage<ID, OP, C>>,
    Self: Sized,
{
    type Error: Error;

    /// Insert a group state into the store.
    fn insert(
        &self,
        id: &ID,
        group: &GroupState<ID, OP, C, RS, ORD, Self>,
    ) -> Result<(), Self::Error>;

    /// Get a group's state from the store.
    #[allow(clippy::type_complexity)]
    fn get(&self, id: &ID) -> Result<Option<GroupState<ID, OP, C, RS, ORD, Self>>, Self::Error>;
}
