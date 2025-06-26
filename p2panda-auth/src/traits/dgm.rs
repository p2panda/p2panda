// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error;

use crate::group::{Access, GroupMember};
use crate::traits::{IdentityHandle, OperationId, Ordering};

/// Decentralised group membership (DGM) API for managing membership of a single group.
pub trait GroupMembership<ID, OP, C, GS, ORD>
where
    ID: IdentityHandle,
    OP: OperationId,
    ORD: Ordering<ID, OP, Self::Action>,
{
    type State;
    type Action;
    type Error: Error;

    /// Initialise the group state.
    fn init(
        my_id: ID,
        group_id: ID,
        store: GS,
        orderer: ORD::State,
    ) -> Result<Self::State, Self::Error>;

    /// Creates a new group, returning the updated state and the creation operation message.
    fn create(
        y: Self::State,
        initial_members: Vec<(GroupMember<ID>, Access<C>)>,
    ) -> Result<(Self::State, ORD::Message), Self::Error>;

    /// Initialise the group by processing a remotely-authored `create` message.
    ///
    /// The group state must first be initialised by calling `init()` before this function is
    /// called. The `group_id` can be extracted from the operation itself.
    fn create_from_remote(
        y: Self::State,
        remote_operation: ORD::Message,
    ) -> Result<Self::State, Self::Error>;

    /// Add a member to the group.
    ///
    /// The updated state is returned, as well as the `add` operation. The operation should be
    /// shared with remote peers so they can update their group state accordingly.
    fn add(
        y: Self::State,
        adder: ID,
        added: ID,
        access: Access<C>,
    ) -> Result<(Self::State, ORD::Message), Self::Error>;

    /// Removes a member from the group.
    fn remove(
        y: Self::State,
        remover: ID,
        removed: ID,
    ) -> Result<(Self::State, ORD::Message), Self::Error>;

    /// Promote a member to the given access level.
    fn promote(
        y: Self::State,
        promoter: ID,
        promoted: ID,
        access: Access<C>,
    ) -> Result<(Self::State, ORD::Message), Self::Error>;

    /// Demote a member to the given access level.
    fn demote(
        y: Self::State,
        demoter: ID,
        demoted: ID,
        access: Access<C>,
    ) -> Result<(Self::State, ORD::Message), Self::Error>;
}
