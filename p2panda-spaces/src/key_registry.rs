// SPDX-License-Identifier: MIT OR Apache-2.0

use std::convert::Infallible;

use p2panda_encryption::crypto::x25519::PublicKey;
use p2panda_encryption::key_bundle::LongTermKeyBundle;
use p2panda_encryption::traits::{IdentityRegistry, PreKeyRegistry};
use serde::{Deserialize, Serialize};

use crate::ActorId;

#[derive(Debug)]
pub struct KeyRegistry;

impl KeyRegistry {
    pub fn init() -> KeyRegistryState {
        KeyRegistryState {}
    }
}

// @TODO: Make this RC-able with interior mutability.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KeyRegistryState {}

impl IdentityRegistry<ActorId, KeyRegistryState> for KeyRegistry {
    type Error = Infallible; // @TODO

    fn identity_key(y: &KeyRegistryState, id: &ActorId) -> Result<Option<PublicKey>, Self::Error> {
        todo!()
    }
}

impl PreKeyRegistry<ActorId, LongTermKeyBundle> for KeyRegistry {
    type State = KeyRegistryState;

    type Error = Infallible; // @TODO

    fn key_bundle(
        y: Self::State,
        id: &ActorId,
    ) -> Result<(Self::State, Option<LongTermKeyBundle>), Self::Error> {
        todo!()
    }
}
