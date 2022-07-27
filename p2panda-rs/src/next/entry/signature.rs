// SPDX-License-Identifier: AGPL-3.0-or-later

use std::hash::Hash as StdHash;

use bamboo_rs_core_ed25519_yasmf::Signature as BambooSignature;

/// Wrapper type around bytes representing an Ed25519 signature.
#[derive(Debug, Clone, Eq, PartialEq, StdHash)]
pub struct Signature(Vec<u8>);

impl Signature {
    /// Returns signature as bytes.
    pub fn into_bytes(&self) -> Vec<u8> {
        self.0.clone()
    }
}

impl From<BambooSignature<&[u8]>> for Signature {
    fn from(signature: BambooSignature<&[u8]>) -> Self {
        Self(signature.0.to_owned())
    }
}

impl From<&[u8]> for Signature {
    fn from(bytes: &[u8]) -> Self {
        Self(bytes.to_owned())
    }
}
