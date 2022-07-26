// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Display;
use std::hash::Hash as StdHash;

use serde::{Deserialize, Serialize};

use crate::hash::Hash;
use crate::serde::{deserialize_hex, serialize_hex};

/// Wrapper type for Bamboo entry bytes.
///
/// This struct can be used to deserialize an hex-encoded string into bytes when using a
/// human-readable encoding format. No validation is applied whatsoever, except of checking if it
/// is a valid hex-string.
///
/// To validate these bytes use the `decode_entry` method to apply all checks and to get an `Entry`
/// instance. Read the module-level documentation for more information.
#[derive(Clone, Debug, PartialEq, Eq, StdHash, Serialize, Deserialize)]
pub struct EncodedEntry(
    #[serde(serialize_with = "serialize_hex", deserialize_with = "deserialize_hex")] Vec<u8>,
);

impl EncodedEntry {
    /// Generates and returns hash of encoded entry.
    pub fn hash(&self) -> Hash {
        Hash::new_from_bytes(self.0)
    }

    /// Returns entry as bytes.
    pub fn into_bytes(&self) -> Vec<u8> {
        self.0
    }

    /// Returns payload size (number of bytes) of total encoded entry.
    pub fn size(&self) -> u64 {
        self.0.len() as u64
    }
}

impl Display for EncodedEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

impl From<&[u8]> for EncodedEntry {
    fn from(bytes: &[u8]) -> Self {
        Self(bytes.to_owned())
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

// @TODO: Move this to decode, encode tests
/* #[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::convert::TryInto;

    use proptest::prelude::prop;
    use rstest::rstest;
    use rstest_reuse::apply;

    use crate::entry::{sign_and_encode, EncodedEntry, Entry};
    use crate::identity::KeyPair;
    use crate::operation::EncodedOperation;
    use crate::test_utils::constants::{test_fields, HASH, PRIVATE_KEY};
    use crate::test_utils::fixtures::{
        entry_signed_encoded, entry_signed_encoded_unvalidated, key_pair, operation,
        operation_encoded, operation_fields, random_hash,
    };
    use crate::test_utils::templates::many_valid_entries;

    #[rstest]
    fn test_entry_signed(entry_signed_encoded: EncodedEntry, key_pair: KeyPair) {
        let verification = KeyPair::verify(
            key_pair.public_key(),
            &entry_signed_encoded.unsigned_bytes(),
            &entry_signed_encoded.signature(),
        );
        assert!(verification.is_ok(), "{:?}", verification.unwrap_err())
    }

    #[rstest]
    fn test_size(entry_signed_encoded: EncodedEntry) {
        let size: usize = entry_signed_encoded.size().try_into().unwrap();
        assert_eq!(size, entry_signed_encoded.to_bytes().len())
    }

    #[rstest]
    fn test_payload_hash(entry_signed_encoded: EncodedEntry, operation_encoded: EncodedOperation) {
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
        assert!(EncodedEntry::new(&entry_signed_encoded_unvalidated).is_ok());
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
            EncodedEntry::new(&entry_signed_encoded_unvalidated)
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
            let result = EncodedEntry::new(s);

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
            let result = EncodedEntry::new(&encoded_entry);

            assert!(result.is_err())
        }
    }
} */
