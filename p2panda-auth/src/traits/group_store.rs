// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error;

use super::IdentityHandle;

/// API for global group store.
pub trait GroupStore<ID>
where
    ID: IdentityHandle,
{
    type Group;
    type Error: Error;

    /// Insert a group state into the store.
    fn insert(&self, id: &ID, group: &Self::Group) -> Result<(), Self::Error>;

    /// Get a group's state from the store.
    fn get(&self, id: &ID) -> Result<Option<Self::Group>, Self::Error>;
}
