// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::{TryFrom, TryInto};
use std::hash::Hash as StdHash;

use arrayvec::ArrayVec;
use bamboo_rs_core_ed25519_yasmf::entry::is_lipmaa_required;
use bamboo_rs_core_ed25519_yasmf::signature::ED25519_SIGNATURE_SIZE;
use bamboo_rs_core_ed25519_yasmf::YasmfHash;
use ed25519_dalek::ed25519::Signature;
use serde::{Deserialize, Serialize};

use crate::entry::EntrySignedError;
use crate::hash::{Blake3ArrayVec, Hash, HASH_SIZE};
use crate::identity::Author;
use crate::operation::EncodedOperation;
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

    /// Returns the hash of the payload of this entry.
    pub fn payload_hash(&self) -> Hash {
        let bamboo_entry: BambooEntry = self.into();
        // Unwrap because we know it was already validated on creating the p2panda entry
        bamboo_entry.payload_hash.try_into().unwrap()
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

    /// Returns only those bytes of a signed entry that don't contain the signature.
    ///
    /// EntrySigned contains both a signature as well as the bytes that were signed. In order to
    /// verify the signature you need access to only the bytes that were signed.
    pub fn unsigned_bytes(&self) -> Vec<u8> {
        let bytes = self.to_bytes();
        let signature_offset = bytes.len() - SIGNATURE_SIZE;
        bytes[..signature_offset].into()
    }

    /// Returns payload size (number of bytes) of total encoded entry.
    pub fn size(&self) -> u64 {
        self.0.len() as u64 / 2
    }

    /// Takes an [`EncodedOperation`] and validates it against the operation hash encoded in this
    /// `EntrySigned`.
    ///
    /// Returns a result containing the [`EncodedOperation`] or an [`EntrySignedError`] if the
    /// operation hashes didn't match.
    pub fn validate_operation(
        &self,
        operation_encoded: &EncodedOperation,
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
        let entry_bytes = self.to_bytes();
        let bamboo_entry: bamboo_rs_core_ed25519_yasmf::Entry<&[u8], &[u8]> =
            entry_bytes.as_slice().try_into()?;

        if is_lipmaa_required(bamboo_entry.seq_num)
            && bamboo_entry.backlink == bamboo_entry.lipmaa_link
        {
            return Err(EntrySignedError::BacklinkAndSkiplinkIdentical);
        };

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::convert::TryInto;

    use proptest::prelude::prop;
    use rstest::rstest;
    use rstest_reuse::apply;

    use crate::entry::{sign_and_encode, Entry, EntrySigned};
    use crate::identity::KeyPair;
    use crate::operation::EncodedOperation;
    use crate::test_utils::constants::{test_fields, HASH, PRIVATE_KEY};
    use crate::test_utils::fixtures::{
        entry_signed_encoded, entry_signed_encoded_unvalidated, key_pair, operation,
        operation_encoded, operation_fields, random_hash,
    };
    use crate::test_utils::templates::many_valid_entries;

    #[rstest]
    fn test_entry_signed(entry_signed_encoded: EntrySigned, key_pair: KeyPair) {
        let verification = KeyPair::verify(
            key_pair.public_key(),
            &entry_signed_encoded.unsigned_bytes(),
            &entry_signed_encoded.signature(),
        );
        assert!(verification.is_ok(), "{:?}", verification.unwrap_err())
    }

    #[rstest]
    fn test_size(entry_signed_encoded: EntrySigned) {
        let size: usize = entry_signed_encoded.size().try_into().unwrap();
        assert_eq!(size, entry_signed_encoded.to_bytes().len())
    }

    #[rstest]
    fn test_payload_hash(entry_signed_encoded: EntrySigned, operation_encoded: EncodedOperation) {
        let expected_payload_hash = operation_encoded.hash();
        assert_eq!(entry_signed_encoded.payload_hash(), expected_payload_hash)
    }

    #[rstest]
    #[case::valid_first_entry(entry_signed_encoded_unvalidated(
        1,
        1,
        None,
        None,
        Some(operation(Some(operation_fields(test_fields())), None, None)),
        key_pair(PRIVATE_KEY)
    ))]
    #[case::valid_entry_with_backlink(entry_signed_encoded_unvalidated(
        2,
        1,
        Some(random_hash()),
        None,
        Some(operation(Some(operation_fields(test_fields())), None, None)),
        key_pair(PRIVATE_KEY)
    ))]
    #[case::valid_entry_with_skiplink_and_backlink(entry_signed_encoded_unvalidated(
        13,
        1,
        Some(random_hash()),
        Some(random_hash()),
        Some(operation(Some(operation_fields(test_fields())), None, None)),
        key_pair(PRIVATE_KEY)
    ))]
    #[case::skiplink_ommitted_when_sam_as_backlink(entry_signed_encoded_unvalidated(
        14,
        1,
        Some(random_hash()),
        None,
        Some(operation(Some(operation_fields(test_fields())), None, None)),
        key_pair(PRIVATE_KEY)
    ))]
    fn validate(#[case] entry_signed_encoded_unvalidated: String) {
        assert!(EntrySigned::new(&entry_signed_encoded_unvalidated).is_ok());
    }

    #[rstest]
    #[case::empty_string("", "Bytes to decode had length of 0")]
    #[case::invalid_hex_string("123456789Z", "invalid hex encoding in entry")]
    #[case::another_invalid_hex_string(":{][[5£$%*(&*££  ++`/.", "invalid hex encoding in entry")]
    #[case::seq_number_zero(
        entry_signed_encoded_unvalidated(
            0,
            1,
            None,
            None,
            Some(operation(Some(operation_fields(test_fields())), None, None)),
            key_pair(PRIVATE_KEY)
        ),
        "Entry sequence must be larger than 0 but was 0"
    )]
    #[case::should_not_have_skiplink(
        entry_signed_encoded_unvalidated(
            1,
            1,
            None,
            Some(random_hash()),
            Some(operation(Some(operation_fields(test_fields())), None, None)),
            key_pair(PRIVATE_KEY)
        ),
        "Could not decode payload hash DecodeError"
    )]
    #[case::should_not_have_backlink(
        entry_signed_encoded_unvalidated(
            1,
            1,
            Some(random_hash()),
            None,
            Some(operation(Some(operation_fields(test_fields())), None, None)),
            key_pair(PRIVATE_KEY)
        ),
        "Could not decode payload hash DecodeError"
    )]
    #[case::should_not_have_backlink_or_skiplink(
        entry_signed_encoded_unvalidated(
                1,
                1,
                Some(HASH.parse().unwrap()),
                Some(HASH.parse().unwrap()),
                Some(operation(Some(operation_fields(test_fields())), None, None))
,
            key_pair(PRIVATE_KEY)
        ),
        "Could not decode payload hash DecodeError"
    )]
    #[case::missing_backlink(
        entry_signed_encoded_unvalidated(
            2,
            1,
            None,
            None,
            Some(operation(Some(operation_fields(test_fields())), None, None)),
            key_pair(PRIVATE_KEY)
        ),
        "Could not decode backlink yamf hash: DecodeError"
    )]
    #[case::missing_skiplink(
        entry_signed_encoded_unvalidated(
            8,
            1,
            Some(random_hash()),
            None,
            Some(operation(Some(operation_fields(test_fields())), None, None)),
            key_pair(PRIVATE_KEY)
        ),
        "Could not decode backlink yamf hash: DecodeError"
    )]
    #[case::should_not_include_skiplink(
        entry_signed_encoded_unvalidated(
            14,
            1,
            Some(HASH.parse().unwrap()),
            Some(HASH.parse().unwrap()),
            Some(operation(Some(operation_fields(test_fields())), None, None)),
            key_pair(PRIVATE_KEY)
        ),
        "Could not decode payload hash DecodeError"
    )]
    #[case::payload_hash_and_size_missing(
        entry_signed_encoded_unvalidated(
            14,
            1,
            Some(random_hash()),
            Some(HASH.parse().unwrap()),
            None,
            key_pair(PRIVATE_KEY)
        ),
        "Could not decode payload hash DecodeError"
    )]
    #[case::skiplink_and_backlink_should_be_unique(
        entry_signed_encoded_unvalidated(
            13,
            1,
            Some(HASH.parse().unwrap()),
            Some(HASH.parse().unwrap()),
            Some(operation(Some(operation_fields(test_fields())), None, None)),
            key_pair(PRIVATE_KEY)
        ),
        "backlink and skiplink are identical"
    )]
    fn correct_errors_on_invalid_entries(
        #[case] entry_signed_encoded_unvalidated: String,
        #[case] expected_error_message: &str,
    ) {
        assert_eq!(
            EntrySigned::new(&entry_signed_encoded_unvalidated)
                .unwrap_err()
                .to_string(),
            expected_error_message
        );
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

    proptest! {
        #[test]
        fn non_standard_strings_dont_crash(ref s in "\\PC*") {
            let result = EntrySigned::new(s);

            assert!(result.is_err())
        }
    }

    proptest! {
        #[test]
        fn partially_correct_strings_dont_crash(
            ref author in "[0-9a-f]{64}|[*]{0}",
            ref log_id in prop::num::u64::ANY,
            ref seq_num in prop::num::u64::ANY,
            ref skiplink in "[0-9a-f]{68}|[*]{0}",
            ref backlink in "[0-9a-f]{68}|[*]{0}",
            ref payload_size in prop::num::u64::ANY,
            ref payload_hash in "[0-9a-f]{68}|[*]{0}",
            ref signature in "[0-9a-f]{68}|[*]{0}"
        ) {
            let encoded_entry = "0".to_string()
                + author
                + log_id.to_string().as_str()
                + seq_num.to_string().as_str()
                + skiplink
                + backlink
                + payload_size.to_string().as_str()
                + payload_hash
                + signature;
            let result = EntrySigned::new(&encoded_entry);

            assert!(result.is_err())
        }
    }
}
