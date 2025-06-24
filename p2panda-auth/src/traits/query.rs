// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;
use std::error::Error;
use std::fmt::Debug;

use crate::group::Access;

use serde::{Deserialize, Serialize};

/// Interface for querying group membership and access levels.
pub trait GroupMembershipQuery<ID, OP, C> {
    //type State: Clone + Debug + Serialize + for<'a> Deserialize<'a>;
    type State;
    type Error: Error;

    /// Query the current access level of the given member.
    fn access(y: &Self::State, member: &ID) -> Result<Access<C>, Self::Error>;

    /// Query group membership from the perspective of the given viewer.
    fn members(y: &Self::State, viewer: &ID) -> Result<HashSet<ID>, Self::Error>;

    /// Return `true` if the given ID is an active member of the group.
    fn is_member(y: &Self::State, possible_member: &ID) -> bool;

    /// Return `true` if the given ID was an active member of the group.
    ///
    /// This represents a member who has been removed from the group.
    fn was_member(y: &Self::State, possible_member: &ID) -> bool;

    /// Return `true` if the given member is currently assigned the `Pull` access level.
    fn is_puller(y: &Self::State, member: &ID) -> bool;

    /// Return `true` if the given member is currently assigned the `Read` access level.
    fn is_reader(y: &Self::State, member: &ID) -> bool;

    /// Return `true` if the given member is currently assigned the `Write` access level.
    fn is_writer(y: &Self::State, member: &ID) -> bool;

    /// Return `true` if the given member is currently assigned the `Manage` access level.
    fn is_manager(y: &Self::State, member: &ID) -> bool;
}
