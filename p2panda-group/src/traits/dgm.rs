// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;
use std::error::Error;
use std::fmt::Debug;

use serde::{Deserialize, Serialize};

pub trait AckedGroupMembership<ID, OP> {
    type State: Clone + Debug + Serialize + for<'a> Deserialize<'a>;

    type Error: Error;

    // TODO: Reconsider init etc.
    fn from_state(my_id: ID, y: Self::State) -> Result<Self::State, Self::Error>;

    // TODO: Reconsider init etc.
    fn create(my_id: ID, initial_members: &[ID]) -> Result<Self::State, Self::Error>;

    fn add(
        y: Self::State,
        adder: ID,
        added: ID,
        message_id: OP,
    ) -> Result<Self::State, Self::Error>;

    fn remove(
        y: Self::State,
        remover: ID,
        removed: &ID,
        message_id: OP,
    ) -> Result<Self::State, Self::Error>;

    fn members_view(y: &Self::State, viewer: &ID) -> Result<HashSet<ID>, Self::Error>;

    fn ack(y: Self::State, acker: ID, message_id: OP) -> Result<Self::State, Self::Error>;

    fn is_add(y: &Self::State, message_id: OP) -> bool;

    fn is_remove(y: &Self::State, message_id: OP) -> bool;
}
