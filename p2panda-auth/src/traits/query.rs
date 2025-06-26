// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;
use std::error::Error;

use crate::group::Access;

/// Interface for querying group membership and access levels.
pub trait GroupMembershipQuery<ID, OP, C> {
    type State;
    type Error: Error;

    /// Query the current access level of the given member.
    fn access(y: &Self::State, member: &ID) -> Result<Access<C>, Self::Error>;

    /// Query group membership.
    fn member_ids(y: &Self::State) -> HashSet<ID>;

    /// Return `true` if the given ID is an active member of the group.
    fn is_member(y: &Self::State, possible_member: &ID) -> bool;

    /// Return `true` if the given member is currently assigned the `Pull` access level.
    fn is_puller(y: &Self::State, member: &ID) -> bool;

    /// Return `true` if the given member is currently assigned the `Read` access level.
    fn is_reader(y: &Self::State, member: &ID) -> bool;

    /// Return `true` if the given member is currently assigned the `Write` access level.
    fn is_writer(y: &Self::State, member: &ID) -> bool;

    /// Return `true` if the given member is currently assigned the `Manage` access level.
    fn is_manager(y: &Self::State, member: &ID) -> bool;
}
