// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryInto;

use bamboo_rs_core_ed25519_yasmf::decode;

use crate::entry::error::DecodeEntryError;
use crate::entry::validate::validate_signature;
use crate::entry::{EncodedEntry, Entry};

/// Method to validate and decode an entry.
///
/// The following validation steps are applied:
///
///     1. Check correct Bamboo encoding as per specification (#E2)
///     2. Check if back- and skiplinks are correctly set for given sequence number (#E3)
///     3. Verify signature (#E5)
///
/// Please note: This method does almost all validation checks required as per specification to
/// make sure the entry is well-formed and correctly signed, with two exceptions:
///
///     1. This is NOT checking for the log integrity as this requires knowledge about other
///        entries / some sort of persistence layer. Use the `validate_log_integrity` method
///        manually to check this as well. (#E4)
///     2. This is NOT checking the payload integrity and authenticity. (#E6)
///
/// Check out the `decode_operation_with_entry` method in the `operation` module if you're
/// interested in full verification of both entries and operations.
pub fn decode_entry(entry_encoded: &EncodedEntry) -> Result<Entry, DecodeEntryError> {
    // Decode the bamboo entry as per specification. This checks if the encoding is correct plus
    // performs a similar check as we do with `validate_links` (#E2 and #E3)
    let bamboo_entry = decode(&entry_encoded.into_bytes())?;

    // Convert from external crate type to our `Entry` struct
    let entry: Entry = bamboo_entry.try_into()?;

    // Check the signature (#E5)
    validate_signature(&entry)?;

    Ok(entry)
}
