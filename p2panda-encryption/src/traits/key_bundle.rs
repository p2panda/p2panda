// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::crypto::x25519::PublicKey;
use crate::key_bundle::{KeyBundleError, OneTimePreKeyId};

/// Key bundle holding data to establish a X3DH key-agreement.
pub trait KeyBundle {
    fn identity_key(&self) -> &PublicKey;

    fn signed_prekey(&self) -> &PublicKey;

    fn onetime_prekey(&self) -> Option<&PublicKey>;

    fn onetime_prekey_id(&self) -> Option<OneTimePreKeyId>;

    fn verify(&self) -> Result<(), KeyBundleError>;
}
