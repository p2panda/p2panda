// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error;

use crate::crypto::x25519::PublicKey;

pub type OneTimeKeyId = u64;

// TODO: Add lifetime.
pub trait KeyBundle {
    type Error: Error;

    fn identity_key(&self) -> &PublicKey;

    fn signed_prekey(&self) -> &PublicKey;

    fn onetime_prekey(&self) -> Option<&PublicKey>;

    fn onetime_prekey_id(&self) -> Option<OneTimeKeyId>;

    fn verify(&self) -> Result<(), Self::Error>;
}
