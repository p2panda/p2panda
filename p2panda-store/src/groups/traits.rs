// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error;

use p2panda_auth::group::GroupCrdtState;
use p2panda_auth::traits::{Conditions, Operation as AuthOperation};
use p2panda_core::{Hash, VerifyingKey};

pub type GroupsStoreState<M, C> = GroupCrdtState<VerifyingKey, Hash, M, C>;

/// Trait describing API for setting and getting groups CRDT state by id.
pub trait GroupsStore<M, C>
where
    M: AuthOperation<VerifyingKey, Hash, C>,
    C: Conditions,
{
    type Error: Error;

    /// Set state for a specified groups instance.
    fn set_groups_state_tx(
        &self,
        id: Hash,
        state: &GroupsStoreState<M, C>,
    ) -> impl Future<Output = Result<(), Self::Error>>;

    /// Get state for a specified groups instance.
    fn get_groups_state_tx(
        &self,
        id: Hash,
    ) -> impl Future<Output = Result<Option<GroupsStoreState<M, C>>, Self::Error>>;
}
