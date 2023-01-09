// SPDX-License-Identifier: AGPL-3.0-or-later

//! Collection of low-level validation methods for entries.
//!
//! You will not find methods here to check the encoding of Bamboo entries, as this is handled
//! inside the external bamboo-rs crate.
use crate::entry::error::ValidateEntryError;
use crate::entry::traits::{AsEncodedEntry, AsEntry};
use crate::entry::{Entry, Signature};
use crate::hash::Hash;
use crate::identity::{KeyPair, PublicKey};
use crate::operation::EncodedOperation;

/// Checks if backlink- and skiplink are correctly set for the given sequence number (#E3).
///
/// First entries do not contain any links. Every other entry has to contain a back- and skiplink
/// unless they are equal, in which case the skiplink must be omitted.
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
    }?;

    if entry.is_skiplink_required() && entry.backlink() == entry.skiplink() {
        return Err(ValidateEntryError::BacklinkAndSkiplinkIdentical);
    }

    Ok(())
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
    entry: &impl AsEntry,
    skiplink: Option<(&impl AsEntry, &Hash)>,
    backlink: Option<(&impl AsEntry, &Hash)>,
) -> Result<(), ValidateEntryError> {
    if let Some((link, link_hash)) = skiplink {
        // Is the claimed link entry part of the same log?
        if entry.log_id() != link.log_id() {
            return Err(ValidateEntryError::WrongSkiplinkLogId(
                entry.log_id().as_u64(),
                link.log_id().as_u64(),
            ));
        }

        if let Some(entry_link) = entry.skiplink() {
            // Is the claimed hash matching with what is in the log?
            // Unwrap here as we know this skiplink exists
            if entry_link != link_hash {
                return Err(ValidateEntryError::WrongSkiplinkHash);
            }
        }

        // Are the claimed entries published by the same key?
        if entry.public_key() != link.public_key() {
            return Err(ValidateEntryError::WrongSkiplinkAuthor);
        }
    };

    if let Some((link, link_hash)) = backlink {
        // Is the claimed link entry part of the same log?
        if entry.log_id() != link.log_id() {
            return Err(ValidateEntryError::WrongBacklinkLogId(
                entry.log_id().as_u64(),
                link.log_id().as_u64(),
            ));
        }

        if let Some(entry_link) = entry.backlink() {
            // Is the claimed hash matching with what is in the log?
            // Unwrap here as we know this backlink exists
            if entry_link != link_hash {
                return Err(ValidateEntryError::WrongBacklinkHash);
            }
        }

        // Are the claimed entries published by the same key?
        if entry.public_key() != link.public_key() {
            return Err(ValidateEntryError::WrongBacklinkAuthor);
        }
    };

    Ok(())
}

/// Checks if the entry is authentic by verifying the public key with the given signature (#E5).
pub fn validate_signature(
    public_key: &PublicKey,
    signature: &Signature,
    encoded_entry: &impl AsEncodedEntry,
) -> Result<(), ValidateEntryError> {
    KeyPair::verify(
        public_key,
        &encoded_entry.unsigned_bytes(),
        &signature.into(),
    )?;

    Ok(())
}

/// Checks if the claimed payload hash and size matches the actual data (#E6).
pub fn validate_payload(
    entry: &impl AsEntry,
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

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::entry::encode::encode_entry;
    use crate::entry::traits::{AsEncodedEntry, AsEntry};
    use crate::entry::{EncodedEntry, Entry, EntryBuilder, SeqNum, Signature};
    use crate::hash::Hash;
    use crate::identity::KeyPair;
    use crate::operation::EncodedOperation;
    use crate::test_utils::fixtures::{
        encoded_entry, encoded_operation, entry, entry_auto_gen_links, key_pair,
    };

    use super::{validate_links, validate_log_integrity, validate_payload, validate_signature};

    #[rstest]
    fn duplicate_back_and_skiplink(
        #[with(4)]
        #[from(entry_auto_gen_links)]
        entry: Entry,
    ) {
        assert!(validate_links(&entry).is_ok());

        // Backlink and skiplink are the same
        let mut invalid_entry = entry.clone();
        invalid_entry.backlink = entry.skiplink().cloned();
        assert!(validate_links(&invalid_entry).is_err());
    }

    #[rstest]
    fn check_signature(
        entry: Entry,
        #[with(1, 99)]
        #[from(encoded_entry)]
        invalid_encoded_entry: EncodedEntry,
    ) {
        let key_pair = KeyPair::new();
        let signature: Signature = key_pair.sign(b"abc").into();
        let encoded_entry = encode_entry(&entry).unwrap();

        // PublicKey does not match signature
        assert!(
            validate_signature(&key_pair.public_key(), entry.signature(), &encoded_entry).is_err()
        );

        // Signature does not match public key
        assert!(validate_signature(entry.public_key(), &signature, &encoded_entry).is_err());

        // Entry bytes are not matching
        assert!(validate_signature(
            entry.public_key(),
            entry.signature(),
            &invalid_encoded_entry
        )
        .is_err());

        // Correct signature
        assert!(validate_signature(entry.public_key(), entry.signature(), &encoded_entry).is_ok());
    }

    #[rstest]
    fn check_payload(
        entry: Entry,
        #[from(encoded_operation)] orig_encoded_operation: EncodedOperation,
        #[with(Some(vec![("other", "fields".into())].into()))] encoded_operation: EncodedOperation,
    ) {
        assert!(validate_payload(&entry, &orig_encoded_operation).is_ok());
        assert!(validate_payload(&entry, &encoded_operation).is_err());
    }

    #[rstest]
    fn check_log_integrity(encoded_operation: EncodedOperation, key_pair: KeyPair) {
        // Create a correct log with 4 entries
        let entry_1 = EntryBuilder::new()
            .sign(&encoded_operation, &key_pair)
            .unwrap();
        let encoded_entry_1 = encode_entry(&entry_1).unwrap();

        let entry_2 = EntryBuilder::new()
            .seq_num(&SeqNum::new(2).unwrap())
            .backlink(&encoded_entry_1.hash())
            .sign(&encoded_operation, &key_pair)
            .unwrap();
        let encoded_entry_2 = encode_entry(&entry_2).unwrap();

        let entry_3 = EntryBuilder::new()
            .seq_num(&SeqNum::new(3).unwrap())
            .backlink(&encoded_entry_2.hash())
            .sign(&encoded_operation, &key_pair)
            .unwrap();
        let encoded_entry_3 = encode_entry(&entry_3).unwrap();

        let entry_4 = EntryBuilder::new()
            .seq_num(&SeqNum::new(4).unwrap())
            .skiplink(&encoded_entry_1.hash())
            .backlink(&encoded_entry_3.hash())
            .sign(&encoded_operation, &key_pair)
            .unwrap();

        // Validate correct log integrity
        assert!(
            validate_log_integrity(&entry_1, None::<(&Entry, &Hash)>, None::<(&Entry, &Hash)>)
                .is_ok()
        );
        assert!(validate_log_integrity(
            &entry_2,
            None::<(&Entry, &Hash)>,
            Some((&entry_1, &encoded_entry_1.hash())),
        )
        .is_ok());
        assert!(validate_log_integrity(
            &entry_3,
            None::<(&Entry, &Hash)>,
            Some((&entry_2, &encoded_entry_2.hash())),
        )
        .is_ok());
        assert!(validate_log_integrity(
            &entry_4,
            Some((&entry_1, &encoded_entry_1.hash())),
            Some((&entry_3, &encoded_entry_3.hash())),
        )
        .is_ok());

        // Validate invalid log integrity
        assert!(validate_log_integrity(
            &entry_2,
            None::<(&Entry, &Hash)>,
            Some((&entry_3, &encoded_entry_3.hash())),
        )
        .is_err());
        assert!(validate_log_integrity(
            &entry_4,
            Some((&entry_3, &encoded_entry_3.hash())),
            Some((&entry_1, &encoded_entry_1.hash())),
        )
        .is_err());
    }
}
