// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::crypto::x25519::PublicKey;
use crate::keybundle::{KeyBundleError, OneTimeKeyId};

pub trait KeyBundle {
    fn identity_key(&self) -> &PublicKey;

    fn signed_prekey(&self) -> &PublicKey;

    fn onetime_prekey(&self) -> Option<&PublicKey>;

    fn onetime_prekey_id(&self) -> Option<OneTimeKeyId>;

    fn verify(&self) -> Result<(), KeyBundleError>;
}
