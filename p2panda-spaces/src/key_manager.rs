// SPDX-License-Identifier: MIT OR Apache-2.0

use std::convert::Infallible;

use p2panda_encryption::Rng;
use p2panda_encryption::crypto::x25519::SecretKey;
use p2panda_encryption::key_bundle::{
    Lifetime, LongTermKeyBundle, OneTimeKeyBundle, OneTimePreKeyId,
};
use p2panda_encryption::key_manager::KeyManagerError;
use p2panda_encryption::traits::{IdentityManager, PreKeyManager};
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub struct KeyManager;

impl KeyManager {
    pub fn init(
        identity_key: &SecretKey,
        lifetime: Lifetime,
        rng: &Rng,
    ) -> Result<KeyManagerState, KeyManagerError> {
        Ok(KeyManagerState {})
    }
}

// @TODO: Make this RC-able with interior mutability.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KeyManagerState {}

impl IdentityManager<KeyManagerState> for KeyManager {
    fn identity_secret(y: &KeyManagerState) -> &SecretKey {
        todo!()
    }
}

impl PreKeyManager for KeyManager {
    type State = KeyManagerState;

    type Error = Infallible; // @TODO

    fn prekey_secret(y: &Self::State) -> &SecretKey {
        todo!()
    }

    fn rotate_prekey(
        y: Self::State,
        lifetime: Lifetime,
        rng: &Rng,
    ) -> Result<Self::State, Self::Error> {
        todo!()
    }

    fn prekey_bundle(y: &Self::State) -> LongTermKeyBundle {
        todo!()
    }

    fn generate_onetime_bundle(
        y: Self::State,
        rng: &Rng,
    ) -> Result<(Self::State, OneTimeKeyBundle), Self::Error> {
        todo!()
    }

    fn use_onetime_secret(
        y: Self::State,
        id: OneTimePreKeyId,
    ) -> Result<(Self::State, Option<SecretKey>), Self::Error> {
        todo!()
    }
}
