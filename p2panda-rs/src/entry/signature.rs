// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt;
use std::hash::Hash as StdHash;

use bamboo_rs_core_ed25519_yasmf::Signature as BambooSignature;
use ed25519_dalek::Signature as Ed25519Signature;

/// Ed25519 signature.
#[derive(Copy, Clone, Debug)]
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

impl From<&Ed25519Signature> for Signature {
    fn from(signature: &Ed25519Signature) -> Self {
        Self(signature.to_owned())
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

impl PartialEq for Signature {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for Signature {}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(self.into_bytes()))
    }
}

#[cfg(test)]
impl Signature {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self(Ed25519Signature::from_bytes(bytes).unwrap())
    }
}
