// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error;

/// Trait describing API for setting and getting groups CRDT state by id.
pub trait GroupsStore<ID, S> {
    type Error: Error;

    fn set_state(&self, id: &ID, state: &S) -> impl Future<Output = Result<(), Self::Error>>;

    fn get_state(&self, id: &ID) -> impl Future<Output = Result<Option<S>, Self::Error>>;
}
