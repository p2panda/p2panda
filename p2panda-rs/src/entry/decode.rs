// SPDX-License-Identifier: AGPL-3.0-or-later

//! Methods to decode an entry.
//!
//! To derive an `Entry` from bytes or a hexadecimal string, use the `EncodedEntry` struct and
//! apply the `decode_entry` method, which allows you to decode the encoded entry into the final
//! `Entry` instance.
//!
//! ```text
//!             ┌────────────┐                         ┌─────┐
//!  bytes ───► │EncodedEntry│ ────decode_entry()────► │Entry│
//!             └────────────┘                         └─────┘
//! ```
use bamboo_rs_core_ed25519_yasmf::decode;

use crate::entry::error::DecodeEntryError;
use crate::entry::validate::{validate_links, validate_signature};
use crate::entry::{EncodedEntry, Entry};

/// Method to decode an entry.
///
/// In this process the following validation steps are applied:
///
/// 1. Check correct Bamboo encoding as per specification (#E2)
/// 2. Check if back- and skiplinks are correctly set for given sequence number (#E3)
/// 3. Verify signature (#E5)
///
/// Please note: This method does almost all validation checks required as per specification to
/// make sure the entry is well-formed and correctly signed, with two exceptions:
///
/// 1. This is NOT checking for the log integrity as this requires knowledge about other entries /
///    some sort of persistence layer. Use the `validate_log_integrity` method manually to check
///    this as well. (#E4)
/// 2. This is NOT checking the payload integrity and authenticity. (#E6)
///
/// Check out the `decode_operation_with_entry` method in the `operation` module if you're
/// interested in full verification of both entries and operations.
pub fn decode_entry(entry_encoded: &EncodedEntry) -> Result<Entry, DecodeEntryError> {
    let bytes = entry_encoded.into_bytes();

    // Decode the bamboo entry as per specification (#E2)
    let bamboo_entry = decode(&bytes)?;

    // Convert from external crate type to our `Entry` struct
    let entry: Entry = bamboo_entry.into();

    // Validate links (#E3). The bamboo-rs crate does check for valid links but not if back- &
    // skiplinks are identical (this is optional but we enforce it)
    validate_links(&entry)?;

    // Check the signature (#E5)
    validate_signature(entry.public_key(), entry.signature(), entry_encoded)?;

    Ok(entry)
}

#[cfg(test)]
mod tests {
    use std::convert::TryInto;

    use rstest::rstest;
    use rstest_reuse::apply;

    use crate::entry::encode::encode_entry;
    use crate::entry::{EncodedEntry, Entry};
    use crate::identity::KeyPair;
    use crate::operation::EncodedOperation;
    use crate::test_utils::constants::{HASH, PRIVATE_KEY};
    use crate::test_utils::fixtures::{
        create_operation_with_schema, encoded_entry, encoded_operation, entry,
        entry_signed_encoded_unvalidated, key_pair, random_hash, Fixture,
    };
    use crate::test_utils::templates::version_fixtures;

    use super::decode_entry;

    #[rstest]
    fn test_entry_signed(entry: Entry) {
        let encoded_entry = encode_entry(&entry).unwrap();

        let verification = KeyPair::verify(
            &entry.public_key().into(),
            &encoded_entry.unsigned_bytes(),
            &entry.signature().into(),
        );

        assert!(verification.is_ok(), "{:?}", verification.unwrap_err())
    }

    #[rstest]
    fn test_size(encoded_entry: EncodedEntry) {
        let size: usize = encoded_entry.size().try_into().unwrap();
        assert_eq!(size, encoded_entry.into_bytes().len())
    }

    #[rstest]
    fn test_payload_hash(entry: Entry, encoded_operation: EncodedOperation) {
        let expected_payload_hash = encoded_operation.hash();
        assert_eq!(entry.payload_hash(), &expected_payload_hash)
    }

    #[rstest]
    #[case::valid_first_entry(entry_signed_encoded_unvalidated(
        1,
        1,
        None,
        None,
        Some(create_operation_with_schema()),
        key_pair(PRIVATE_KEY)
    ))]
    #[case::valid_entry_with_backlink(entry_signed_encoded_unvalidated(
        2,
        1,
        Some(random_hash()),
        None,
        Some(create_operation_with_schema()),
        key_pair(PRIVATE_KEY)
    ))]
    #[case::valid_entry_with_skiplink_and_backlink(entry_signed_encoded_unvalidated(
        13,
        1,
        Some(random_hash()),
        Some(random_hash()),
        Some(create_operation_with_schema()),
        key_pair(PRIVATE_KEY)
    ))]
    #[case::skiplink_ommitted_when_sam_as_backlink(entry_signed_encoded_unvalidated(
        14,
        1,
        Some(random_hash()),
        None,
        Some(create_operation_with_schema()),
        key_pair(PRIVATE_KEY)
    ))]
    fn decode_correct_entries(#[case] entry_encoded_unvalidated: EncodedEntry) {
        assert!(decode_entry(&entry_encoded_unvalidated).is_ok());
    }

    #[rstest]
    #[case::empty_string(EncodedEntry::from_str(""), "Bytes to decode had length of 0")]
    #[case::seq_number_zero(
        entry_signed_encoded_unvalidated(
            0,
            1,
            None,
            None,
            Some(create_operation_with_schema()),
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
            Some(create_operation_with_schema()),
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
            Some(create_operation_with_schema()),
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
            Some(create_operation_with_schema()),
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
            Some(create_operation_with_schema()),
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
            Some(create_operation_with_schema()),
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
            Some(create_operation_with_schema()),
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
            Some(create_operation_with_schema()),
            key_pair(PRIVATE_KEY)
        ),
        "backlink and skiplink are identical"
    )]
    fn correct_errors_on_invalid_entries(
        #[case] unverified_encoded_entry: EncodedEntry,
        #[case] expected_error_message: &str,
    ) {
        assert_eq!(
            decode_entry(&unverified_encoded_entry)
                .unwrap_err()
                .to_string(),
            expected_error_message
        );
    }

    #[apply(version_fixtures)]
    fn decode_fixture_entry(#[case] fixture: Fixture) {
        // Decode `EncodedEntry` fixture
        let entry = decode_entry(&fixture.entry_encoded).unwrap();

        // Decoded `Entry` values should match fixture `Entry` values
        assert_eq!(entry.log_id(), fixture.entry.log_id());
        assert_eq!(entry.seq_num(), fixture.entry.seq_num());
        assert_eq!(entry.skiplink(), fixture.entry.skiplink());
        assert_eq!(entry.backlink(), fixture.entry.backlink());
    }
}
