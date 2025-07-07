// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error;
use std::fmt::Debug;

use serde::{Deserialize, Serialize};

use crate::crypto::Rng;
use crate::crypto::x25519::SecretKey;
use crate::key_bundle::{Lifetime, LongTermKeyBundle, OneTimeKeyBundle, OneTimePreKeyId};

/// Manages our own identity secret.
pub trait IdentityManager<Y> {
    fn identity_secret(y: &Y) -> &SecretKey;
}

/// Manages our own pre-key secrets to generate public key bundles.
pub trait PreKeyManager {
    type State: Debug + Serialize + for<'a> Deserialize<'a>;

    type Error: Error;

    fn prekey_secret(y: &Self::State) -> &SecretKey;

    fn rotate_prekey(
        y: Self::State,
        lifetime: Lifetime,
        rng: &Rng,
    ) -> Result<Self::State, Self::Error>;

    fn prekey_bundle(y: &Self::State) -> LongTermKeyBundle;

    fn generate_onetime_bundle(
        y: Self::State,
        rng: &Rng,
    ) -> Result<(Self::State, OneTimeKeyBundle), Self::Error>;

    fn use_onetime_secret(
        y: Self::State,
        id: OneTimePreKeyId,
    ) -> Result<(Self::State, Option<SecretKey>), Self::Error>;
}
