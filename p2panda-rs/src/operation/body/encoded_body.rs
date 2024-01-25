// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Display;
use std::hash::Hash as StdHash;

use serde::{Deserialize, Serialize};

use crate::hash::Hash;
use crate::serde::{deserialize_hex, serialize_hex};

#[derive(Clone, Debug, PartialEq, Eq, StdHash, Serialize, Deserialize)]
pub struct EncodedBody(
    #[serde(serialize_with = "serialize_hex", deserialize_with = "deserialize_hex")] Vec<u8>,
);

impl EncodedBody {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self(bytes.to_owned())
    }

    pub fn hash(&self) -> Hash {
        Hash::new_from_bytes(&self.0)
    }

    pub fn into_bytes(&self) -> Vec<u8> {
        self.0.clone()
    }

    pub fn size(&self) -> u64 {
        self.0.len() as u64
    }
}

impl Display for EncodedBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(&self.0))
    }
}

#[cfg(any(feature = "test-utils", test))]
impl EncodedBody {
    /// Returns a new instance of `EncodedBody` for testing.
    pub fn new(bytes: &[u8]) -> EncodedBody {
        Self(bytes.to_owned())
    }

    /// Converts hexadecimal string into bytes and returns as a new instance of `EncodedBody`.
    pub fn from_hex(value: &str) -> EncodedBody {
        let bytes = hex::decode(value).expect("invalid hexadecimal value");
        Self(bytes)
    }
}
