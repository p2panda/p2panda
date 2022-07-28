// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Display;
use std::hash::Hash as StdHash;

use serde::{Deserialize, Serialize};

use crate::next::hash::Hash;
use crate::next::serde::{deserialize_hex, serialize_hex};

/// Wrapper type for Bamboo entry bytes.
///
/// This struct can be used to deserialize an hex-encoded string into bytes when using a
/// human-readable encoding format. No validation is applied whatsoever, except of checking if it
/// is a valid hex-string (#E1).
///
/// To validate these bytes use the `decode_entry` method to apply all checks and to get an `Entry`
/// instance. Read the module-level documentation for more information.
#[derive(Clone, Debug, PartialEq, Eq, StdHash, Serialize, Deserialize)]
pub struct EncodedEntry(
    #[serde(serialize_with = "serialize_hex", deserialize_with = "deserialize_hex")] Vec<u8>,
);

impl EncodedEntry {
    /// Returns new `EncodedEntry` instance from given bytes.
    ///
    /// This does not apply any validation and should only be used in methods where all checks have
    /// taken place before.
    // @TODO: Check pub(crate) visibility
    pub(crate) fn from_bytes(bytes: &[u8]) -> Self {
        Self(bytes.to_owned())
    }

    /// Generates and returns hash of encoded entry.
    pub fn hash(&self) -> Hash {
        Hash::new_from_bytes(&self.0)
    }

    /// Returns entry as bytes.
    pub fn into_bytes(&self) -> Vec<u8> {
        self.0.clone()
    }

    /// Returns payload size (number of bytes) of total encoded entry.
    pub fn size(&self) -> u64 {
        self.0.len() as u64
    }
}

impl Display for EncodedEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(&self.0))
    }
}

#[cfg(test)]
impl EncodedEntry {
    pub fn new(bytes: &[u8]) -> EncodedEntry {
        Self(bytes.to_owned())
    }

    pub fn from_str(value: &str) -> EncodedEntry {
        let bytes = hex::decode(value).expect("invalid hexadecimal value");
        Self(bytes)
    }
}
