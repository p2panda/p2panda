// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Display;
use std::hash::Hash as StdHash;

use bamboo_rs_core_ed25519_yasmf::ED25519_SIGNATURE_SIZE;
use serde::{Deserialize, Serialize};

use crate::entry::traits::AsEncodedEntry;
use crate::hash::Hash;
use crate::serde::{deserialize_hex, serialize_hex};
use crate::storage_provider::traits::EntryWithOperation;

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
    #[serde(serialize_with = "serialize_hex", deserialize_with = "deserialize_hex")] Vec<u8>,
);

impl EncodedEntry {
    /// Returns new `EncodedEntry` instance from given bytes.
    ///
    /// This does not apply any validation and should only be used in methods where all checks have
    /// taken place before.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self(bytes.to_owned())
    }

    /// Returns only those bytes of a signed entry that don't contain the signature.
    ///
    /// Encoded entries contains both a signature as well as the bytes that were signed. In order
    /// to verify the signature you need access to only the bytes that were used during signing.
    pub fn unsigned_bytes(&self) -> Vec<u8> {
        let bytes = self.into_bytes();
        let signature_offset = bytes.len() - SIGNATURE_SIZE;
        bytes[..signature_offset].into()
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

impl<T: EntryWithOperation> From<T> for EncodedEntry {
    fn from(entry: T) -> Self {
        EncodedEntry(entry.into_bytes())
    }
}

#[cfg(any(feature = "test-utils", test))]
impl EncodedEntry {
    pub fn new(bytes: &[u8]) -> EncodedEntry {
        Self(bytes.to_owned())
    }

    pub fn new_from_str(value: &str) -> EncodedEntry {
        let bytes = hex::decode(value).expect("invalid hexadecimal value");
        Self(bytes)
    }
}
