// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::entry::{decode_entry, Entry, EntrySigned, LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::Author;
use crate::operation::{Operation, OperationEncoded, OperationId};
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
    pub entry_bytes: EntrySigned,

    /// Hash of Bamboo entry data.
    pub entry_hash: Hash,

    /// Used log for this entry.
    pub log_id: LogId,

    /// Payload of entry, can be deleted.
    pub payload_bytes: Option<OperationEncoded>,

    /// Hash of payload data.
    pub payload_hash: OperationId,

    /// Sequence number of this entry.
    pub seq_num: SeqNum,
}

impl StorageEntry {
    /// Get the decoded entry.
    pub fn entry_decoded(&self) -> Entry {
        // Unwrapping as validation occurs in constructor.
        decode_entry(&self.entry_signed(), self.operation_encoded().as_ref()).unwrap()
    }

    /// Get the encoded entry.
    pub fn entry_signed(&self) -> EntrySigned {
        self.entry_bytes.clone()
    }

    /// Get the encoded operation.
    pub fn operation_encoded(&self) -> Option<OperationEncoded> {
        self.payload_bytes.clone()
    }
}

/// Implement `AsStorageEntry` trait for `StorageEntry`
impl AsStorageEntry for StorageEntry {
    type AsStorageEntryError = EntryStorageError;

    fn new(
        entry: &EntrySigned,
        operation: &OperationEncoded,
    ) -> Result<Self, Self::AsStorageEntryError> {
        let entry_decoded = decode_entry(entry, None).unwrap();

        let entry = StorageEntry {
            author: entry.author(),
            entry_bytes: entry.clone(),
            entry_hash: entry.hash(),
            log_id: entry_decoded.log_id().to_owned(),
            payload_bytes: Some(operation.clone()),
            payload_hash: entry.payload_hash().into(),
            seq_num: entry_decoded.seq_num().to_owned(),
        };

        entry.validate()?;
        Ok(entry)
    }

    fn author(&self) -> Author {
        self.entry_signed().author()
    }

    fn hash(&self) -> Hash {
        self.entry_signed().hash()
    }

    fn entry_bytes(&self) -> Vec<u8> {
        self.entry_signed().to_bytes()
    }

    fn backlink_hash(&self) -> Option<Hash> {
        self.entry_decoded().backlink_hash().cloned()
    }

    fn skiplink_hash(&self) -> Option<Hash> {
        self.entry_decoded().skiplink_hash().cloned()
    }

    fn seq_num(&self) -> SeqNum {
        *self.entry_decoded().seq_num()
    }

    fn log_id(&self) -> LogId {
        *self.entry_decoded().log_id()
    }

    fn operation(&self) -> Operation {
        let operation_encoded = self.operation_encoded().unwrap();
        Operation::from(&operation_encoded)
    }
}

impl Validate for StorageEntry {
    type Error = ValidationError;

    fn validate(&self) -> Result<(), Self::Error> {
        self.entry_signed().validate()?;
        if let Some(operation) = self.operation_encoded() {
            operation.validate()?;
        }
        decode_entry(&self.entry_signed(), self.operation_encoded().as_ref())?;
        Ok(())
    }
}
