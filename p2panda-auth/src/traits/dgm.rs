// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;
use std::error::Error;
use std::fmt::Debug;

use serde::{Deserialize, Serialize};

use crate::group::{Access, GroupMember};
use crate::traits::{IdentityHandle, OperationId, Ordering};

// TODO: Maybe `GroupApi` or `GroupQuery...` something something.

/// Decentralised group membership (DGM) API for managing membership of a single group.
pub trait GroupMembership<ID, OP, C, GS, ORD>
where
    ID: IdentityHandle,
    OP: OperationId,
    ORD: Ordering<ID, OP, Self::Action>,
{
    //type State: Clone + Debug + Serialize + for<'a> Deserialize<'a>;
    type State;
    type Action;
    type Error: Error;

    // TODO(glyph): Do we have any concept of destroying a group?

    /// Creates a new group, returning the updated state and the creation operation message.
    fn create(
        my_id: ID,
        group_id: ID,
        initial_members: Vec<(GroupMember<ID>, Access<C>)>,
        store: GS,
        orderer: ORD::State,
    ) -> Result<(Self::State, ORD::Message), Self::Error>;

    // TODO: Sometimes we want to "create" a group that was started elsewhere (from another peer).
    // `from_welcome()` or `from_message()` or something...
    // Need to think about this..
    // Two-step process or one?
    //
    // Initialise the group by processing a remotely-authored `create` message.
    //fn create_from_remote()

    /// Adds a member to the group.
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
