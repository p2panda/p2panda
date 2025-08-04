// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;
use std::convert::Infallible;

use serde::{Deserialize, Serialize};

use crate::types::{ActorId, OperationId};

// @TODO: It's strange that Serialize & Deserialize (along with other traits)
// are required here. It's only a requirement because EncryptionGroupMembership
// is a generic parameter on EncryptionDirectMessage.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EncryptionGroupMembership {}

// @TODO: Maybe put `serde` features behind a feature-flag in `p2panda-encryption`?
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EncryptionMembershipState {
    pub(crate) members: HashSet<ActorId>,
}

impl p2panda_encryption::traits::GroupMembership<ActorId, OperationId>
    for EncryptionGroupMembership
{
    type State = EncryptionMembershipState;

    type Error = Infallible; // @TODO

    fn create(_my_id: ActorId, initial_members: &[ActorId]) -> Result<Self::State, Self::Error> {
        Ok(EncryptionMembershipState {
            members: HashSet::from_iter(initial_members.iter().cloned()),
        })
    }

    fn from_welcome(_my_id: ActorId, _y: Self::State) -> Result<Self::State, Self::Error> {
        todo!()
    }

    fn add(
        _y: Self::State,
        _adder: ActorId,
        _added: ActorId,
        _operation_id: OperationId,
    ) -> Result<Self::State, Self::Error> {
        todo!()
    }

    fn remove(
        _y: Self::State,
        _remover: ActorId,
        _removed: &ActorId,
        _operation_id: OperationId,
    ) -> Result<Self::State, Self::Error> {
        todo!()
    }

    fn members(y: &Self::State) -> Result<HashSet<ActorId>, Self::Error> {
        Ok(y.members.clone())
    }
}
