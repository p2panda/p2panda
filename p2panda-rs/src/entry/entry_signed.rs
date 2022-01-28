// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::{TryFrom, TryInto};
use std::hash::{Hash as StdHash, Hasher};

use arrayvec::ArrayVec;
use bamboo_rs_core_ed25519_yasmf::signature::ED25519_SIGNATURE_SIZE;
use bamboo_rs_core_ed25519_yasmf::{Entry as BambooEntry, YasmfHash};
use ed25519_dalek::ed25519::Signature;
use serde::{Deserialize, Serialize};

use crate::entry::EntrySignedError;
use crate::hash::{Blake3ArrayVec, HASH_SIZE};
use crate::identity::Author;
use crate::operation::OperationEncoded;
use crate::Validate;

/// Size of p2panda entries' signatures.
pub const SIGNATURE_SIZE: usize = ED25519_SIGNATURE_SIZE;

/// Bamboo entry bytes represented in hex encoding format.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntrySigned(String);

impl EntrySigned {
    /// Validates and wraps encoded entry string into a new `EntrySigned` instance.
    pub fn new(value: &str) -> Result<Self, EntrySignedError> {
        let inner = Self(value.to_owned());
        inner.validate()?;
        Ok(inner)
    }

    /// Returns `Author` who signed this entry.
    pub fn author(&self) -> Author {
        // Unwrap as we already validated entry
        let entry: BambooEntry<ArrayVec<[u8; HASH_SIZE]>, ArrayVec<[u8; SIGNATURE_SIZE]>> =
            self.try_into().unwrap();
        Author::try_from(entry.author).unwrap()
    }

    /// Returns Ed25519 signature of this entry.
    pub fn signature(&self) -> Signature {
        // Unwrap as we already validated entry and know it contains a signature
        let entry: BambooEntry<ArrayVec<[u8; HASH_SIZE]>, ArrayVec<[u8; SIGNATURE_SIZE]>> =
            self.try_into().unwrap();

        // Convert into Ed25519 Signature instance
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
    /// `EntrySigned`, returns a result containing the [`OperationEncoded`] or an
    /// [`EntrySignedError`] if the operation hashes didn't match.
    pub fn validate_operation(
        &self,
        operation_encoded: &OperationEncoded,
    ) -> Result<(), EntrySignedError> {
        // Convert to Entry from bamboo_rs_core_ed25519_yasmf first
        let entry: BambooEntry<ArrayVec<[u8; HASH_SIZE]>, ArrayVec<[u8; SIGNATURE_SIZE]>> =
            self.into();

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
impl From<&EntrySigned> for BambooEntry<ArrayVec<[u8; HASH_SIZE]>, ArrayVec<[u8; SIGNATURE_SIZE]>> {
    fn from(signed_entry: &EntrySigned) -> Self {
        let entry_bytes = signed_entry.clone().to_bytes();
        let entry_ref: BambooEntry<&[u8], &[u8]> = entry_bytes.as_slice().try_into().unwrap();
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

/// Validate the integrity of signed Bamboo entries.
impl Validate for EntrySigned {
    type Error = EntrySignedError;

    fn validate(&self) -> Result<(), Self::Error> {
        hex::decode(&self.0).map_err(|_| EntrySignedError::InvalidHexEncoding)?;
        Ok(())
    }
}

/// Implement `Hash` trait for `EntrySigned` to make it a hashable type.
///
/// Bamboo entry hashes are computed on a concrete byte encoding, defined in the [Bamboo
/// specification].
///
/// [Bamboo specification]: https://github.com/AljoschaMeyer/bamboo#encoding
impl StdHash for EntrySigned {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.to_bytes().hash(state);
    }
}

impl PartialEq for EntrySigned {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for EntrySigned {}
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
    fn sign_and_encode_roundtrip(#[case] entry: Entry, key_pair: KeyPair) {
        let entry_first_encoded = sign_and_encode(&entry, &key_pair).unwrap();
        let mut hash_map = HashMap::new();
        let key_value = "Value identified by a hash".to_string();
        hash_map.insert(&entry_first_encoded, key_value.clone());
        let key_value_retrieved = hash_map.get(&entry_first_encoded).unwrap().to_owned();
        assert_eq!(key_value, key_value_retrieved)
    }
}
