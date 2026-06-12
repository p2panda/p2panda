// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;
use std::convert::Infallible;

use p2panda_encryption::traits::GroupMembership;
use serde::{Deserialize, Serialize};

use crate::{MemberId, OperationId};

/// Placeholder for DGM implementation which satisfies required trait interfaces in
/// p2panda-encryption. Most methods perform no actual actions as group management is handled by
/// p2panda-auth.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncryptionGroupMembership;

#[derive(Clone, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptionMembershipState {
    pub(crate) members: HashSet<MemberId>,
}

impl GroupMembership<MemberId, OperationId> for EncryptionGroupMembership {
    type State = EncryptionMembershipState;

    type Error = Infallible;

    fn create(_my_id: MemberId, initial_members: &[MemberId]) -> Result<Self::State, Self::Error> {
        Ok(EncryptionMembershipState {
            members: HashSet::from_iter(initial_members.iter().cloned()),
        })
    }

    fn from_welcome(_my_id: MemberId, y: Self::State) -> Result<Self::State, Self::Error> {
        Ok(y)
    }

    fn add(
        y: Self::State,
        _adder: MemberId,
        _added: MemberId,
        _operation_id: OperationId,
    ) -> Result<Self::State, Self::Error> {
        // The DGM state is already updated manually before this method is called so no action is
        // required.
        Ok(y)
    }

    fn remove(
        y: Self::State,
        _remover: MemberId,
        _removed: &MemberId,
        _operation_id: OperationId,
    ) -> Result<Self::State, Self::Error> {
        // The DGM state is already updated manually before this method is called so no action is
        // required.
        Ok(y)
    }

    fn members(y: &Self::State) -> Result<HashSet<MemberId>, Self::Error> {
        Ok(y.members.clone())
    }
}
