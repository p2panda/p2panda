// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error;

/// Trait describing API for setting and getting groups CRDT state by id.
pub trait GroupsStore<ID, S> {
    type Error: Error;

    /// Set state for a specified groups instance.
    fn set_groups_state_tx(
        &self,
        id: &ID,
        state: &S,
    ) -> impl Future<Output = Result<(), Self::Error>>;

    /// Get state for a specified groups instance.
    fn get_groups_state_tx(&self, id: &ID) -> impl Future<Output = Result<Option<S>, Self::Error>>;
}
