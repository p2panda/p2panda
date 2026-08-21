// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::hash::Hash;
use crate::operation::PayloadSize;

/// Body of a p2panda operation containing arbitrary bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Body(Vec<u8>);

impl Body {
    /// Construct a body from a byte slice.
    pub fn from_bytes(bytes: impl AsRef<[u8]>) -> Self {
        Self(bytes.as_ref().to_vec())
    }

    /// Access the underlying body bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.0.clone()
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// BLAKE3 hash of the body bytes.
    pub fn hash(&self) -> Hash {
        Hash::digest(&self.0)
    }

    /// Size of body bytes.
    pub fn size(&self) -> PayloadSize {
        self.0.len() as PayloadSize
    }

    #[cfg(any(test, feature = "test_utils"))]
    pub fn to_hex(&self) -> String {
        hex::encode(&self.0)
    }
}

impl AsRef<[u8]> for Body {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<&[u8]> for Body {
    fn from(value: &[u8]) -> Self {
        Body::from_bytes(value)
    }
}

impl From<Vec<u8>> for Body {
    fn from(value: Vec<u8>) -> Self {
        Body(value)
    }
}
