// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;
use std::fmt::Debug;

use crate::Access;
use crate::group::GroupMember;
use crate::traits::{IdentityHandle, OperationId};

/// Decentralised group membership (DGM) API for managing membership of a single group.
pub trait Groups<ID, OP, C, MSG>
where
    ID: IdentityHandle,
    OP: OperationId,
{
    type Error: Debug;

    /// Creates a new group, returning the updated state and the creation operation message.
    ///
    /// The group state must first be initialised by calling `init()` before this function is
    /// called.
    fn create(
        &mut self,
        group_id: ID,
        initial_members: Vec<(GroupMember<ID>, Access<C>)>,
    ) -> Result<MSG, Self::Error>;

    /// Process a remotely-authored group action message.
    fn receive_from_remote(
        &mut self,
        remote_operation: MSG,
    ) -> Result<(), Self::Error>;

    /// Add a member to the group.
    ///
    /// The updated state is returned, as well as the `add` operation. The operation should be
    /// shared with remote peers so they can update their group state accordingly.
    fn add(
        &mut self,
        group_id: ID,
        adder: ID,
        added: ID,
        access: Access<C>,
    ) -> Result<MSG, Self::Error>;

    /// Removes a member from the group.
    fn remove(
        &mut self,
        group_id: ID,
        remover: ID,
        removed: ID,
    ) -> Result<MSG, Self::Error>;

    /// Promote a member to the given access level.
    fn promote(
        &mut self,
        group_id: ID,
        promoter: ID,
        promoted: ID,
        access: Access<C>,
    ) -> Result<MSG, Self::Error>;

    /// Demote a member to the given access level.
    fn demote(
        &mut self,
        group_id: ID,
        demoter: ID,
        demoted: ID,
        access: Access<C>,
    ) -> Result<MSG, Self::Error>;
}

/// Interface for querying group membership and access levels.
pub trait GroupMembership<ID, OP, C> {
    type Error: Debug;

    /// Query the current access level of the given member.
    ///
    /// The member is expected to be a "stateless" individual, not a "stateful" group.
    fn access(&self, group_id: ID, member: ID) -> Result<Access<C>, Self::Error>;

    /// Query group membership.
    fn member_ids(&self, group_id: ID) -> Result<HashSet<ID>, Self::Error>;

    /// Return `true` if the given ID is an active member of the group.
    fn is_member(&self, group_id: ID, member: ID) -> Result<bool, Self::Error>;

    /// Return `true` if the given member is currently assigned the `Pull` access level.
    fn is_puller(&self, group_id: ID, member: ID) -> Result<bool, Self::Error>;

    /// Return `true` if the given member is currently assigned the `Read` access level.
    fn is_reader(&self, group_id: ID, member: ID) -> Result<bool, Self::Error>;

    /// Return `true` if the given member is currently assigned the `Write` access level.
    fn is_writer(&self, group_id: ID, member: ID) -> Result<bool, Self::Error>;

    /// Return `true` if the given member is currently assigned the `Manage` access level.
    fn is_manager(&self, group_id: ID, member: ID) -> Result<bool, Self::Error>;
}
