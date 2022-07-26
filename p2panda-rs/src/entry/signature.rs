// SPDX-License-Identifier: AGPL-3.0-or-later

use bamboo_rs_core_ed25519_yasmf::Signature as BambooSignature;

#[derive(Debug, Clone, PartialEq)]
pub struct Signature(Vec<u8>);

impl Signature {
    pub fn to_bytes(&self) -> Vec<u8> {
        self.0
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
