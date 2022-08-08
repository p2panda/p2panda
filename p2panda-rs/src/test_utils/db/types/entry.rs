// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::entry::decode::decode_entry;
use crate::entry::{EncodedEntry, Entry, LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::Author;
use crate::storage_provider::error::EntryStorageError;
use crate::storage_provider::traits::AsStorageEntry;

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
    pub payload_hash: Hash,

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
            payload_hash: entry_decoded.payload_hash().to_owned(),
            seq_num: entry_decoded.seq_num().to_owned(),
        };

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
