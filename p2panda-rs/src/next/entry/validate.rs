// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::next::entry::error::ValidateEntryError;
use crate::next::entry::Entry;
use crate::next::operation::EncodedOperation;

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
/// 1. Are the claimed backlink and skiplink entries part of the same log?
/// 2. Are the claimed backlinks and skiplinks published by the same key?
/// 3. Are the claimed backlink and skiplink hashes matching with what is in the log?
///
/// This method requires knowledge about other entries. Use this together with your storage
/// provider implementation.
pub fn validate_log_integrity(
    entry: &Entry,
    skiplink_entry: Option<&Entry>,
    backlink_entry: Option<&Entry>,
) -> Result<(), ValidateEntryError> {
    // @TODO
    Ok(())
}

/// Checks if the entry is authentic by verifying the public key with the given signature (#E5).
pub fn validate_signature(entry: &Entry) -> Result<(), ValidateEntryError> {
    // @TODO
    Ok(())
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

// @TODO: Needs refactoring
/* #[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::entry::{LogId, SeqNum};
    use crate::hash::Hash;
    use crate::operation::{Operation, OperationFields, OperationValue};
    use crate::schema::SchemaId;
    use crate::test_utils::fixtures::{entry, schema};
    use crate::Validate;

    use super::Entry;

    #[rstest]
    fn validation(schema: SchemaId) {
        // Prepare sample values
        let mut fields = OperationFields::new();
        fields
            .add("test", OperationValue::Text("Hello".to_owned()))
            .unwrap();
        let operation = Operation::new_create(schema, fields).unwrap();
        let backlink = Hash::new_from_bytes(vec![7, 8, 9]).unwrap();

        // The first entry in a log doesn't need and cannot have references to previous entries
        assert!(Entry::new(
            &LogId::default(),
            Some(&operation),
            None,
            None,
            &SeqNum::new(1).unwrap()
        )
        .is_ok());

        // Try to pass them over anyways, it will be invalidated
        assert!(Entry::new(
            &LogId::default(),
            Some(&operation),
            Some(&backlink),
            Some(&backlink),
            &SeqNum::new(1).unwrap()
        )
        .is_err());

        // Any following entry requires backlinks
        assert!(Entry::new(
            &LogId::default(),
            Some(&operation),
            Some(&backlink),
            Some(&backlink),
            &SeqNum::new(2).unwrap()
        )
        .is_ok());

        // We can omit the skiplink here as it is the same as the backlink
        assert!(Entry::new(
            &LogId::default(),
            Some(&operation),
            None,
            Some(&backlink),
            &SeqNum::new(2).unwrap()
        )
        .is_ok());

        // We need a backlink here
        assert!(Entry::new(
            &LogId::default(),
            Some(&operation),
            None,
            None,
            &SeqNum::new(2).unwrap()
        )
        .is_err());
    }

    #[rstest]
    pub fn validate_many(entry: Entry) {
        assert!(entry.validate().is_ok())
    }
} */
