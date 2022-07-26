// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::entry::{Entry, ValidateEntryError};
use crate::operation::EncodedOperation;

pub fn verify_payload(entry: &Entry, payload: &EncodedOperation) -> Result<(), ValidateEntryError> {
    if entry.payload_hash() != &payload.hash() {
        return Err(ValidateEntryError::PayloadHashMismatch);
    }

    if entry.payload_size() != payload.size() {
        return Err(ValidateEntryError::PayloadSizeMismatch);
    }

    Ok(())
}
