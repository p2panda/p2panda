// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::next::entry::decode::decode_entry;
use crate::next::entry::{EncodedEntry, Entry, LogId, SeqNum};
use crate::next::hash::Hash;
use crate::next::identity::Author;
use crate::next::operation::decode::decode_operation;
use crate::next::operation::plain::PlainOperation;
use crate::next::operation::{EncodedOperation, Operation, OperationId, VerifiedOperation};
use crate::storage_provider::errors::EntryStorageError;
use crate::storage_provider::traits::AsStorageEntry;
use crate::storage_provider::ValidationError;
use crate::Validate;

/// A struct which represents an entry and operation pair in storage as a concatenated string.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StorageEntry {
    /// Public key of the author.
    pub author: Author,

    /// Actual Bamboo entry data.
    pub entry_bytes: EncodedEntry,

    /// Hash of Bamboo entry data.
    pub entry_hash: Hash,

    /// Used log for this entry.
    pub log_id: LogId,

    /// Hash of payload data.
    pub payload_hash: OperationId,

    /// Sequence number of this entry.
    pub seq_num: SeqNum,
}

impl StorageEntry {
    /// Get the decoded entry.
    pub fn entry_decoded(&self) -> Entry {
        // Unwrapping as validation occurs in constructor.
        decode_entry(&self.entry_signed()).unwrap()
    }

    /// Get the encoded entry.
    pub fn entry_signed(&self) -> EncodedEntry {
        self.entry_bytes.clone()
    }
    //
    //     /// Get the encoded operation.
    //     pub fn operation_encoded(&self) -> Option<EncodedOperation> {
    //         self.payload_bytes.clone()
    //     }
}

impl AsStorageEntry for StorageEntry {
    type AsStorageEntryError = EntryStorageError;

    fn new(entry: &EncodedEntry) -> Result<Self, Self::AsStorageEntryError> {
        let entry_decoded = decode_entry(entry).unwrap();

        let entry = StorageEntry {
            author: entry_decoded.public_key().to_owned(),
            entry_bytes: entry.clone(),
            entry_hash: entry.hash(),
            log_id: entry_decoded.log_id().to_owned(),
            payload_hash: entry_decoded.payload_hash().to_owned().into(),
            seq_num: entry_decoded.seq_num().to_owned(),
        };

        entry.validate()?;
        Ok(entry)
    }

    fn author(&self) -> Author {
        self.entry_decoded().public_key().to_owned()
    }

    fn hash(&self) -> Hash {
        self.entry_signed().hash()
    }

    fn entry_bytes(&self) -> Vec<u8> {
        self.entry_signed().into_bytes()
    }

    fn backlink_hash(&self) -> Option<Hash> {
        self.entry_decoded().backlink().cloned()
    }

    fn skiplink_hash(&self) -> Option<Hash> {
        self.entry_decoded().skiplink().cloned()
    }

    fn seq_num(&self) -> SeqNum {
        *self.entry_decoded().seq_num()
    }

    fn log_id(&self) -> LogId {
        *self.entry_decoded().log_id()
    }
}

impl Validate for StorageEntry {
    type Error = ValidationError;

    fn validate(&self) -> Result<(), Self::Error> {
        // TODO: Maybe we are just gunna remove this, need to think about it still
        Ok(())
    }
}
