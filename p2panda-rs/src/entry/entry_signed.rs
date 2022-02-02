// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::{TryFrom, TryInto};
use std::hash::Hash as StdHash;

use arrayvec::ArrayVec;
use bamboo_rs_core_ed25519_yasmf::signature::ED25519_SIGNATURE_SIZE;
use bamboo_rs_core_ed25519_yasmf::YasmfHash;
use ed25519_dalek::ed25519::Signature;
use serde::{Deserialize, Serialize};

use crate::entry::EntrySignedError;
use crate::hash::{Blake3ArrayVec, Hash, HASH_SIZE};
use crate::identity::Author;
use crate::operation::OperationEncoded;
use crate::Validate;

/// Size of p2panda entries' signatures.
pub const SIGNATURE_SIZE: usize = ED25519_SIGNATURE_SIZE;

/// Bamboo Entry type with regarding byte sizes for hashes and signatures.
type BambooEntry =
    bamboo_rs_core_ed25519_yasmf::Entry<ArrayVec<[u8; HASH_SIZE]>, ArrayVec<[u8; SIGNATURE_SIZE]>>;

/// Bamboo entry bytes represented in hex encoding format.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, StdHash)]
pub struct EntrySigned(String);

impl EntrySigned {
    /// Validates and wraps encoded entry string into a new `EntrySigned` instance.
    pub fn new(value: &str) -> Result<Self, EntrySignedError> {
        let inner = Self(value.to_owned());
        inner.validate()?;
        Ok(inner)
    }

    /// Generates and returns YASMF BLAKE3 hash of encoded entry.
    pub fn hash(&self) -> Hash {
        Hash::new_from_bytes(self.to_bytes()).unwrap()
    }

    /// Returns `Author` who signed this entry.
    pub fn author(&self) -> Author {
        let entry: BambooEntry = self.into();
        Author::try_from(entry.author).unwrap()
    }

    /// Returns Ed25519 signature of this entry.
    pub fn signature(&self) -> Signature {
        let entry: BambooEntry = self.into();

        // Convert into Ed25519 Signature instance and unwrap here since we already checked the
        // signature
        let array_vec = entry.sig.unwrap().0;
        Signature::from_bytes(&array_vec.into_inner().unwrap()).unwrap()
    }

    /// Returns encoded entry as string.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Decodes hex encoding and returns entry as bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        // Unwrap as we already know that the inner value is valid
        hex::decode(&self.0).unwrap()
    }

    /// Returns payload size (number of bytes) of total encoded entry.
    pub fn size(&self) -> u64 {
        self.0.len() as u64 / 2
    }

    /// Takes an [`OperationEncoded`] and validates it against the operation hash encoded in this
    /// `EntrySigned`.
    ///
    /// Returns a result containing the [`OperationEncoded`] or an [`EntrySignedError`] if the
    /// operation hashes didn't match.
    pub fn validate_operation(
        &self,
        operation_encoded: &OperationEncoded,
    ) -> Result<(), EntrySignedError> {
        // Convert to Entry from bamboo_rs_core_ed25519_yasmf first
        let entry: BambooEntry = self.into();

        // Operation hash must match if it doesn't return an error
        let yasmf_hash: YasmfHash<Blake3ArrayVec> = (&operation_encoded.hash()).to_owned().into();
        if yasmf_hash != entry.payload_hash {
            return Err(EntrySignedError::OperationHashMismatch);
        }

        Ok(())
    }
}

/// Converts an `EntrySigned` into a Bamboo Entry to interact with the
/// `bamboo_rs_core_ed25519_yasmf` crate.
impl From<&EntrySigned> for BambooEntry {
    fn from(signed_entry: &EntrySigned) -> Self {
        let entry_bytes = signed_entry.to_bytes();
        // Unwrap as we already validated entry
        let entry_ref = entry_bytes.as_slice().try_into().unwrap();
        bamboo_rs_core_ed25519_yasmf::entry::into_owned(&entry_ref)
    }
}

/// Converts byte sequence into `EntrySigned`.
impl TryFrom<&[u8]> for EntrySigned {
    type Error = EntrySignedError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        Self::new(&hex::encode(bytes))
    }
}

impl Validate for EntrySigned {
    type Error = EntrySignedError;

    /// Validate the integrity of signed Bamboo entries.
    fn validate(&self) -> Result<(), Self::Error> {
        hex::decode(&self.0).map_err(|_| EntrySignedError::InvalidHexEncoding)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rstest::rstest;
    use rstest_reuse::apply;

    use crate::entry::{sign_and_encode, Entry};
    use crate::identity::KeyPair;
    use crate::test_utils::fixtures::key_pair;
    use crate::test_utils::fixtures::templates::many_valid_entries;

    use super::EntrySigned;

    #[test]
    fn validate() {
        // Invalid hex string
        assert!(EntrySigned::new("123456789Z").is_err());
    }

    #[apply(many_valid_entries)]
    fn it_hashes(#[case] entry: Entry, key_pair: KeyPair) {
        let entry_first_encoded = sign_and_encode(&entry, &key_pair).unwrap();
        let mut hash_map = HashMap::new();
        let key_value = "Value identified by a hash".to_string();
        hash_map.insert(&entry_first_encoded, key_value.clone());
        let key_value_retrieved = hash_map.get(&entry_first_encoded).unwrap().to_owned();
        assert_eq!(key_value, key_value_retrieved)
    }
}
