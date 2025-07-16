// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;
use std::convert::Infallible;

use serde::{Deserialize, Serialize};

use crate::{ActorId, OperationId};

pub struct EncryptionGroupMembership {}

// @TODO: Maybe put `serde` features behind a feature-flag in `p2panda-encryption`?
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EncryptionMembershipState {}

impl p2panda_encryption::traits::GroupMembership<ActorId, OperationId>
    for EncryptionGroupMembership
{
    type State = EncryptionMembershipState;

    type Error = Infallible; // @TODO

    fn create(my_id: ActorId, initial_members: &[ActorId]) -> Result<Self::State, Self::Error> {
        todo!()
    }

    fn from_welcome(my_id: ActorId, y: Self::State) -> Result<Self::State, Self::Error> {
        todo!()
    }

    fn add(
        y: Self::State,
        adder: ActorId,
        added: ActorId,
        operation_id: OperationId,
    ) -> Result<Self::State, Self::Error> {
        todo!()
    }

    fn remove(
        y: Self::State,
        remover: ActorId,
        removed: &ActorId,
        operation_id: OperationId,
    ) -> Result<Self::State, Self::Error> {
        todo!()
    }

    fn members(y: &Self::State) -> Result<HashSet<ActorId>, Self::Error> {
        todo!()
    }
}
