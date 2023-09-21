// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Display;
use std::hash::Hash as StdHash;

use serde::{Deserialize, Serialize};

use crate::entry::traits::AsEncodedEntry;
use crate::hash::Hash;
use crate::serde::{deserialize_hex, serialize_hex};

#[derive(Clone, Debug, PartialEq, Eq, StdHash, Serialize, Deserialize)]
pub struct EncodedHeader(
    #[serde(serialize_with = "serialize_hex", deserialize_with = "deserialize_hex")] Vec<u8>,
);

impl EncodedHeader {
    /// Returns new `EncodedHeader` instance from given bytes.
    ///
    /// This does not apply any validation and should only be used in methods where all checks have
    /// taken place before.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self(bytes.to_owned())
    }
}

impl AsEncodedEntry for EncodedHeader {
    /// Generates and returns hash of encoded entry.
    fn hash(&self) -> Hash {
        Hash::new_from_bytes(&self.0)
    }

    /// Returns entry as bytes.
    fn into_bytes(&self) -> Vec<u8> {
        self.0.clone()
    }

    /// Returns payload size (number of bytes) of total encoded entry.
    fn size(&self) -> u64 {
        self.0.len() as u64
    }
}

impl Display for EncodedHeader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(&self.0))
    }
}

#[cfg(any(feature = "test-utils", test))]
impl EncodedHeader {
    /// Returns a new instance of `EncodedHeader` for testing.
    pub fn new(bytes: &[u8]) -> EncodedHeader {
        Self(bytes.to_owned())
    }

    /// Converts hexadecimal string into bytes and returns as a new instance of `EncodedHeader`.
    pub fn from_hex(value: &str) -> EncodedHeader {
        let bytes = hex::decode(value).expect("invalid hexadecimal value");
        Self(bytes)
    }
}
