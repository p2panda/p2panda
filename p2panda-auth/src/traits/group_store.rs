// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error;

use super::IdentityHandle;

/// API for global group store.
pub trait GroupStore<ID>
where
    ID: IdentityHandle,
{
    type State;
    type Group;
    type Error: Error;

    /// Insert a group state into the store.
    fn insert(y: Self::State, id: &ID, group: &Self::Group) -> Result<Self::State, Self::Error>;

    /// Get a group's state from the store.
    fn get(y: &Self::State, id: &ID) -> Result<Option<Self::Group>, Self::Error>;
}
