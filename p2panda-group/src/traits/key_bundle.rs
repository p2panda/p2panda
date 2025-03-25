// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::crypto::x25519::PublicKey;
use crate::two_party::X3DHError;

pub type OneTimeKeyId = u64;

// TODO: Add lifetime.
pub trait KeyBundle {
    fn identity_key(&self) -> &PublicKey;

    fn signed_prekey(&self) -> &PublicKey;

    fn onetime_prekey(&self) -> Option<&PublicKey>;

    fn onetime_prekey_id(&self) -> Option<OneTimeKeyId>;

    fn verify(&self) -> Result<(), X3DHError>;
}
