// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error;

use crate::Access;
use crate::group::GroupMember;
use crate::traits::{IdentityHandle, OperationId, Ordering};

/// Decentralised group membership (DGM) API for managing membership of a single group.
pub trait GroupMembership<ID, OP, C, ORD>
where
    ID: IdentityHandle,
    OP: OperationId,
    // TODO: Do we strictly need the orderer here? Could it rather be a generic message?
    // We might not actually need to know anything about the message type, only in the `Orderer`.
    // In the _implementation_ we'd say it's an `ORD::Message` but not here (move that knowledge
    // into the implementation.
    ORD: Ordering<ID, OP, Self::Action>,
{
    type State;
    type Action;
    type Error: Error;

    /// Creates a new group, returning the updated state and the creation operation message.
    ///
    /// The group state must first be initialised by calling `init()` before this function is
    /// called.
    fn create(
        &self,
        group_id: ID,
        initial_members: Vec<(GroupMember<ID>, Access<C>)>,
    ) -> Result<(Self::State, ORD::Message), Self::Error>;

    /// Initialise the group by processing a remotely-authored `create` message.
    ///
    /// The group state must first be initialised by calling `init()` before this function is
    /// called. The `group_id` is extracted from the operation itself.
    fn create_from_remote(
        &self,
        remote_operation: ORD::Message,
    ) -> Result<Self::State, Self::Error>;

    /// Process a remotely-authored group action message.
    fn receive_from_remote(
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
