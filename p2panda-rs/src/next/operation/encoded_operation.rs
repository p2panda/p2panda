// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Display;
use std::hash::Hash as StdHash;

use serde::{Deserialize, Serialize};

use crate::hash::Hash;
use crate::next::serde::{deserialize_hex, serialize_hex};

/// Wrapper type for operation bytes.
// @TODO: Fix pub(crate) visibility
#[derive(Clone, Debug, PartialEq, Eq, StdHash, Serialize, Deserialize)]
pub struct EncodedOperation(
    #[serde(serialize_with = "serialize_hex", deserialize_with = "deserialize_hex")]
    pub(crate)  Vec<u8>,
);

impl EncodedOperation {
    /// Returns the hash of this operation.
    pub fn hash(&self) -> Hash {
        Hash::new_from_bytes(&self.0)
    }

    /// Returns operation as bytes.
    pub fn into_bytes(&self) -> Vec<u8> {
        self.0.clone()
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0[..]
    }

    /// Returns payload size (number of bytes) of encoded operation.
    pub fn size(&self) -> u64 {
        self.0.len() as u64
    }
}

impl Display for EncodedOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(&self.0))
    }
}

#[cfg(test)]
impl EncodedOperation {
    pub fn new(bytes: &[u8]) -> EncodedOperation {
        Self(bytes.to_owned())
    }

    pub fn from_str(value: &str) -> EncodedOperation {
        let bytes = hex::decode(value).expect("invalid hexadecimal value");
        Self(bytes)
    }
}
