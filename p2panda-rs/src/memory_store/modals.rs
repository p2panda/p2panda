// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryInto;
use std::fmt::Debug;

use crate::document::DocumentId;
use crate::entry::decode_entry;
use crate::entry::EntrySigned;
use crate::entry::LogId;
use crate::entry::SeqNum;
use crate::entry::SIGNATURE_SIZE;
use crate::hash::Hash;
use crate::hash::HASH_SIZE;
use crate::identity::Author;
use crate::operation::OperationEncoded;
use crate::schema::SchemaId;
use arrayvec::ArrayVec;
use bamboo_rs_core_ed25519_yasmf::Entry as BambooEntry;

use super::MemoryStoreError;

#[derive(Debug)]
pub struct LogModal {
    /// Public key of the author.
    author: Author,

    /// Log id used for this document.
    log_id: LogId,

    /// Hash that identifies the document this log is for.
    document: DocumentId,

    /// SchemaId which identifies the schema for operations in this log.
    schema: SchemaId,
}

impl LogModal {
    pub fn new(author: &Author, document: &DocumentId, schema: &SchemaId, log_id: &LogId) -> Self {
        Self {
            author: author.to_owned(),
            log_id: log_id.to_owned(),
            document: document.to_owned(),
            schema: schema.to_owned(),
        }
    }
}

#[derive(Debug)]
pub struct EntryModal {
    /// Public key of the author.
    pub author: Author,

    /// Actual Bamboo entry data.
    pub entry_bytes: String,

    /// Hash of Bamboo entry data.
    pub entry_hash: Hash,

    /// Used log for this entry.
    pub log_id: LogId,

    /// Payload of entry, can be deleted.
    pub payload_bytes: Option<String>,

    /// Hash of payload data.
    pub payload_hash: Hash,

    /// Sequence number of this entry.
    pub seq_num: SeqNum,
}

impl EntryModal {
    pub fn new(
        entry_encoded: &EntrySigned,
        operation_encoded: Option<&OperationEncoded>,
    ) -> Result<Self, MemoryStoreError> {
        let author = entry_encoded.author();
        let entry = decode_entry(&entry_encoded, operation_encoded).unwrap();
        let payload_bytes = match operation_encoded {
            Some(operation_encoded) => Some(operation_encoded.as_str().to_string()),
            None => None,
        };
        let bamboo_entry: BambooEntry<ArrayVec<[u8; HASH_SIZE]>, ArrayVec<[u8; SIGNATURE_SIZE]>> =
            entry_encoded.into();
        let payload_hash = bamboo_entry.payload_hash.try_into().unwrap();

        Ok(Self {
            author,
            entry_bytes: entry_encoded.as_str().into(),
            entry_hash: entry_encoded.hash(),
            log_id: *entry.log_id(),
            payload_bytes,
            payload_hash,
            seq_num: *entry.seq_num(),
        })
    }
}
