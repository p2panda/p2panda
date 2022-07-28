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

use crate::next::entry::error::DecodeEntryError;
use crate::next::entry::validate::validate_signature;
use crate::next::entry::{EncodedEntry, Entry};

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

    // Decode the bamboo entry as per specification. This checks if the encoding is correct plus
    // performs a similar check as we do with `validate_links` (#E2 and #E3)
    let bamboo_entry = decode(&bytes)?;

    // Convert from external crate type to our `Entry` struct
    let entry: Entry = bamboo_entry.into();

    // Check the signature (#E5)
    validate_signature(&entry)?;

    Ok(entry)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use rstest_reuse::apply;

    use crate::next::test_utils::fixtures::Fixture;
    use crate::next::test_utils::templates::version_fixtures;

    use super::decode_entry;

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
