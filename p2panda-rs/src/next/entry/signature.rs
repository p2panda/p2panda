// SPDX-License-Identifier: AGPL-3.0-or-later

use std::hash::Hash as StdHash;

use bamboo_rs_core_ed25519_yasmf::Signature as BambooSignature;
use ed25519_dalek::Signature as Ed25519Signature;

/// Ed25519 signature.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Signature(Ed25519Signature);

impl Signature {
    /// Returns signature as bytes.
    pub fn into_bytes(&self) -> Vec<u8> {
        self.0.to_bytes().to_vec()
    }
}

impl From<BambooSignature<&[u8]>> for Signature {
    fn from(signature: BambooSignature<&[u8]>) -> Self {
        // Unwrap here as the signature from bamboo should already be checked
        Signature(Ed25519Signature::from_bytes(signature.0).unwrap())
    }
}

impl From<Ed25519Signature> for Signature {
    fn from(signature: Ed25519Signature) -> Self {
        Self(signature)
    }
}

impl From<&Signature> for Ed25519Signature {
    fn from(signature: &Signature) -> Self {
        signature.0.to_owned()
    }
}

impl StdHash for Signature {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.into_bytes().hash(state)
    }
}
