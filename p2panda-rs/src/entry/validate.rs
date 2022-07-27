// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::entry::error::ValidateEntryError;
use crate::entry::Entry;
use crate::operation::EncodedOperation;

/// Checks if backlink- and skiplink are correctly set for the given sequence number (#E3).
///
/// First entries do not contain any links. Every other entry has to contain a back- and skiplink
/// unless they are equal, in which case the skiplink can be omitted.
pub fn validate_links(entry: &Entry) -> Result<(), ValidateEntryError> {
    match (
        entry.seq_num().is_first(),
        entry.backlink().is_some(),
        entry.skiplink().is_some(),
        entry.is_skiplink_required(),
    ) {
        (true, false, false, false) => Ok(()),
        (false, true, false, false) => Ok(()),
        (false, true, true, _) => Ok(()),
        (_, _, _, _) => Err(ValidateEntryError::InvalidLinks),
    }
}

/// Checks if entry is correctly placed in its log (#E4).
///
/// The following validation steps are applied:
///
///     1. Are the claimed backlink and skiplink entries part of the same log?
///     2. Are the claimed backlinks and skiplinks published by the same key?
///     3. Are the claimed backlink and skiplink hashes matching with what is in the log?
///
/// This method requires knowledge about other entries. Use this together with your storage
/// provider implementation.
pub fn validate_log_integrity(
    entry: &Entry,
    skiplink_entry: Option<&Entry>,
    backlink_entry: Option<&Entry>,
) -> Result<(), ValidateEntryError> {
    // @TODO
    unimplemented!();
}

/// Checks if the entry is authentic by verifying the public key with the given signature (#E5).
pub fn validate_signature(entry: &Entry) -> Result<(), ValidateEntryError> {
    // @TODO
    unimplemented!();
}

/// Checks if the claimed payload hash and size matches the actual data (#E6).
pub fn validate_payload(
    entry: &Entry,
    payload: &EncodedOperation,
) -> Result<(), ValidateEntryError> {
    if entry.payload_hash() != &payload.hash() {
        return Err(ValidateEntryError::PayloadHashMismatch);
    }

    if entry.payload_size() != payload.size() {
        return Err(ValidateEntryError::PayloadSizeMismatch);
    }

    Ok(())
}
