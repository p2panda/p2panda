// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::entry::decode::decode_entry;
use crate::entry::traits::{AsEncodedEntry, AsEntry};
use crate::entry::{EncodedEntry, Entry, LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::Author;
use crate::operation::EncodedOperation;
use crate::storage_provider::traits::EntryWithOperation;

/// A struct which represents an entry and operation pair in storage as a concatenated string.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StorageEntry {
    /// Public key of the author.
    pub entry: Entry,

    /// Actual Bamboo entry data.
    pub encoded_entry: EncodedEntry,

    /// The entry payload.
    pub payload: Option<EncodedOperation>,
}

impl StorageEntry {
    pub fn new(encoded_entry: &EncodedEntry, operation: Option<&EncodedOperation>) -> Self {
        let entry = decode_entry(encoded_entry).expect("Invalid encoded entry given");
        Self {
            entry,
            encoded_entry: encoded_entry.to_owned(),
            payload: operation.cloned(),
        }
    }
}

impl EntryWithOperation for StorageEntry {
    fn payload(&self) -> Option<&EncodedOperation> {
        self.payload.as_ref()
    }
}

impl AsEntry for StorageEntry {
    fn backlink(&self) -> Option<&Hash> {
        self.entry.backlink()
    }

    fn skiplink(&self) -> Option<&Hash> {
        self.entry.skiplink()
    }

    fn seq_num(&self) -> &SeqNum {
        self.entry.seq_num()
    }

    fn log_id(&self) -> &LogId {
        self.entry.log_id()
    }

    fn public_key(&self) -> &Author {
        self.entry.public_key()
    }

    fn payload_size(&self) -> u64 {
        self.entry.payload_size()
    }

    fn payload_hash(&self) -> &Hash {
        self.entry.payload_hash()
    }

    fn signature(&self) -> &crate::entry::Signature {
        self.entry.signature()
    }
}

impl AsEncodedEntry for StorageEntry {
    fn hash(&self) -> Hash {
        self.encoded_entry.hash()
    }

    fn into_bytes(&self) -> Vec<u8> {
        self.encoded_entry.into_bytes()
    }

    fn size(&self) -> u64 {
        self.encoded_entry.size()
    }
}
