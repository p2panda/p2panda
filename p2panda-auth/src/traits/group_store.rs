// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error;

use super::IdentityHandle;

/// API for global group store.
pub trait GroupStore<ID, G>
where
    ID: IdentityHandle,
{
    type State;
    type Error: Error;

    /// Insert a group state into the store.
    fn insert(y: Self::State, id: &ID, group: &G) -> Result<Self::State, Self::Error>;

    /// Get a group's state from the store.
    fn get(y: &Self::State, id: &ID) -> Result<Option<G>, Self::Error>;
}
