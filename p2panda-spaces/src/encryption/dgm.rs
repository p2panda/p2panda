// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;
use std::convert::Infallible;

use p2panda_core::{Hash, VerifyingKey};
use p2panda_encryption::traits::GroupMembership;
use serde::{Deserialize, Serialize};

/// Placeholder for DGM implementation which satisfies required trait interfaces in
/// p2panda-encryption. Most methods perform no actual actions as group management is handled by
/// p2panda-auth.
// TODO: It's strange that Serialize & Deserialize (along with other traits) are required here. It's
// only a requirement because EncryptionGroupMembership is a generic parameter on
// EncryptionDirectMessage.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EncryptionGroupMembership;

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct EncryptionMembershipState {
    pub(crate) members: HashSet<VerifyingKey>,
}

impl GroupMembership<VerifyingKey, Hash> for EncryptionGroupMembership {
    type State = EncryptionMembershipState;

    type Error = Infallible;

    fn create(
        _my_id: VerifyingKey,
        initial_members: &[VerifyingKey],
    ) -> Result<Self::State, Self::Error> {
        Ok(EncryptionMembershipState {
            members: HashSet::from_iter(initial_members.iter().cloned()),
        })
    }

    fn from_welcome(_my_id: VerifyingKey, y: Self::State) -> Result<Self::State, Self::Error> {
        Ok(y)
    }

    fn add(
        y: Self::State,
        _adder: VerifyingKey,
        _added: VerifyingKey,
        _operation_id: Hash,
    ) -> Result<Self::State, Self::Error> {
        // The DGM state is already updated manually before this method is called so no action is
        // required.
        Ok(y)
    }

    fn remove(
        y: Self::State,
        _remover: VerifyingKey,
        _removed: &VerifyingKey,
        _operation_id: Hash,
    ) -> Result<Self::State, Self::Error> {
        // The DGM state is already updated manually before this method is called so no action is
        // required.
        Ok(y)
    }

    fn members(y: &Self::State) -> Result<HashSet<VerifyingKey>, Self::Error> {
        Ok(y.members.clone())
    }
}
