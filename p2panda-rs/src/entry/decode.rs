// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryInto;

use bamboo_rs_core_ed25519_yasmf::decode;

use crate::entry::error::DecodeEntryError;
use crate::entry::{EncodedEntry, Entry};

/// Method to decode an entry.
pub fn decode_entry(entry_encoded: &EncodedEntry) -> Result<Entry, DecodeEntryError> {
    // Decode the bamboo entry as per specification
    let bamboo_entry = decode(&entry_encoded.into_bytes())?;

    // Convert to our entry struct
    let entry: Entry = bamboo_entry.try_into()?;

    Ok(entry)
}
