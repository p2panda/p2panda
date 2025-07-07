// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;
use std::error::Error;
use std::fmt::Debug;

use serde::{Deserialize, Serialize};

/// Decentralised group membership (DGM) algorithm with acknowledgements.
///
/// Tracking acknowledgements is required for understanding concurrent group operations and
/// handling possible cases where members would otherwise miss out on crucial state to set up their
/// ratchets (see DCGKA implementation and paper for handling concurrency cases for more info).
///
/// This is the DGM interface for p2panda's "message encryption" scheme.
#[cfg(any(test, feature = "message_scheme"))]
pub trait AckedGroupMembership<ID, OP> {
    type State: Clone + Debug + Serialize + for<'a> Deserialize<'a>;

    type Error: Error;

    /// Creates a new group.
    fn create(my_id: ID, initial_members: &[ID]) -> Result<Self::State, Self::Error>;

    /// Processes the received DGM state from a welcome message.
    ///
    /// Implementations might want to take the state of the remote peer (sent via a welcome
    /// message) and only keep their own id. We might receive multiple welcome messages as peers
    /// might add us concurrently, in this case we might want to "merge" the states into one.
    fn from_welcome(y: Self::State, y_welcome: Self::State) -> Result<Self::State, Self::Error>;

    /// Adds a member to the group.
    fn add(
        y: Self::State,
        adder: ID,
        added: ID,
        operation_id: OP,
    ) -> Result<Self::State, Self::Error>;

    /// Removes a member from a group.
    fn remove(
        y: Self::State,
        remover: ID,
        removed: &ID,
        operation_id: OP,
    ) -> Result<Self::State, Self::Error>;

    /// Member acknowledged a group operation.
    fn ack(y: Self::State, acker: ID, operation_id: OP) -> Result<Self::State, Self::Error>;

    /// Returns the list of current members in the group from the perspective of a "viewer".
    ///
    /// Membership operations like adding or removing are only recognized by a member when they
    /// have been explicitly acknowledged by them. This is why different members can have different
    /// "views" on the same group. Note that we are still looking at all of that from our knowledge
    /// horizon aka the messages we could observe on the network.
    fn members_view(y: &Self::State, viewer: &ID) -> Result<HashSet<ID>, Self::Error>;

    /// Returns true if given group operation added a member.
    fn is_add(y: &Self::State, operation_id: OP) -> bool;

    /// Returns true if given group operation removed a member.
    fn is_remove(y: &Self::State, operation_id: OP) -> bool;
}

/// Decentralised group membership (DGM) algorithm.
///
/// This is the DGM interface for p2panda's "data encryption" scheme.
#[cfg(any(test, feature = "data_scheme"))]
pub trait GroupMembership<ID, OP> {
    type State: Clone + Debug + Serialize + for<'a> Deserialize<'a>;

    type Error: Error;

    /// Creates a new group.
    fn create(my_id: ID, initial_members: &[ID]) -> Result<Self::State, Self::Error>;

    /// Processes the received DGM state from a welcome message.
    fn from_welcome(my_id: ID, y: Self::State) -> Result<Self::State, Self::Error>;

    /// Adds a member to the group.
    fn add(
        y: Self::State,
        adder: ID,
        added: ID,
        operation_id: OP,
    ) -> Result<Self::State, Self::Error>;

    /// Removes a member from a group.
    fn remove(
        y: Self::State,
        remover: ID,
        removed: &ID,
        operation_id: OP,
    ) -> Result<Self::State, Self::Error>;

    /// Returns the list of current members in the group.
    fn members(y: &Self::State) -> Result<HashSet<ID>, Self::Error>;
}
