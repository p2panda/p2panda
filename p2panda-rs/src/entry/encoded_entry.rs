// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Display;
use std::hash::Hash as StdHash;

use bamboo_rs_core_ed25519_yasmf::ED25519_SIGNATURE_SIZE;
use serde::{Deserialize, Serialize};

use crate::entry::traits::AsEncodedEntry;
use crate::hash::Hash;
use crate::serde::{deserialize_hex, serialize_hex_bytes};

/// Size of p2panda entries' signatures.
pub const SIGNATURE_SIZE: usize = ED25519_SIGNATURE_SIZE;

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
    #[serde(serialize_with = "serialize_hex_bytes", deserialize_with = "deserialize_hex")] Vec<u8>,
);

impl EncodedEntry {
    /// Returns new `EncodedEntry` instance from given bytes.
    ///
    /// This does not apply any validation and should only be used in methods where all checks have
    /// taken place before.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self(bytes.to_owned())
    }
}

impl AsEncodedEntry for EncodedEntry {
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

impl Display for EncodedEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(&self.0))
    }
}

#[cfg(any(feature = "test-utils", test))]
impl EncodedEntry {
    /// Returns a new instance of `EncodedEntry` for testing.
    pub fn new(bytes: &[u8]) -> EncodedEntry {
        Self(bytes.to_owned())
    }

    /// Converts hexadecimal string into bytes and returns as a new instance of `EncodedEntry`.
    pub fn from_hex(value: &str) -> EncodedEntry {
        let bytes = hex::decode(value).expect("invalid hexadecimal value");
        Self(bytes)
    }
}
