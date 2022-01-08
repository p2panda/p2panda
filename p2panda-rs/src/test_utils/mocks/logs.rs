// SPDX-License-Identifier: AGPL-3.0-or-later

//! Structs which encapsulate log data structures and logic.
//!
//! Much of the logic here looks different from how it will in a real world p2panda application. As
//! more functionality is implemented in the main library this will be replaced with core modules.
//! For these reasons this code is only intended for testing or demo purposes.
use std::convert::TryFrom;

use crate::entry::{decode_entry, EntrySigned, LogId};
use crate::hash::Hash;
use crate::identity::Author;
use crate::operation::{AsOperation, Operation, OperationEncoded};

/// Entry of an append-only which contains an encoded entry and operation.
#[derive(Clone, Debug)]
pub struct LogEntry {
    /// Encoded entry.
    pub entry_encoded: EntrySigned,

    /// Encoded operation.
    pub operation_encoded: OperationEncoded,
}

impl LogEntry {
    /// Create a new log.
    pub fn new(entry_encoded: EntrySigned, operation_encoded: OperationEncoded) -> Self {
        Self {
            entry_encoded,
            operation_encoded,
        }
    }

    /// Get the author of this entry.
    pub fn author(&self) -> String {
        self.entry_encoded.author().as_str().to_string()
    }

    /// Get the hash of this entry.
    pub fn hash(&self) -> Hash {
        self.entry_encoded.hash()
    }

    /// Get the hash of this entry as a string.
    pub fn hash_str(&self) -> String {
        self.entry_encoded.hash().as_str().to_string()
    }

    /// Get the operation from this entry.
    pub fn operation(&self) -> Operation {
        Operation::try_from(&self.operation_encoded).unwrap()
    }

    /// Get the encoded entry from this entry.
    pub fn entry_encoded(&self) -> EntrySigned {
        self.entry_encoded.clone()
    }

    /// Get the encoded operation from this entry.
    pub fn operation_encoded(&self) -> OperationEncoded {
        self.operation_encoded.clone()
    }

    /// Get the previous operation hash for this entry.
    pub fn previous_operations(&self) -> Option<Vec<Hash>> {
        self.operation().previous_operations()
    }
}

/// Tracks the assigment of an author's logs to documents and records their schema.
///
/// This serves as an indexing layer on top of the lower-level bamboo entries. The node updates
/// this data according to what it sees in the newly incoming entries.
#[derive(Debug, Clone)]
pub struct Log {
    /// Public key of the author.
    author: Author,

    /// Log id used for this document.
    log_id: LogId,

    /// Hash that identifies the document this log is for.
    document: Hash,

    /// Schema hash used by author.
    schema: Hash,

    /// The entries in this log.
    entries: Vec<LogEntry>,
}

impl Log {
    /// Create a new log.
    pub fn new(entry_signed: &EntrySigned, operation_encoded: &OperationEncoded) -> Self {
        let entry = decode_entry(entry_signed, Some(operation_encoded)).unwrap();
        Self {
            author: entry_signed.author(),
            log_id: entry.log_id().to_owned(),
            document: entry_signed.hash(),
            schema: entry.operation().unwrap().schema(),
            entries: Vec::new(),
        }
    }

    /// Get the entries from this log.
    pub fn entries(&self) -> Vec<LogEntry> {
        self.entries.to_owned()
    }

    /// Get the author of this log.
    pub fn author(&self) -> Author {
        self.author.to_owned()
    }

    /// Get the id of this log.
    pub fn id(&self) -> LogId {
        self.log_id.to_owned()
    }

    /// Get the schema of this log.
    pub fn schema(&self) -> Hash {
        self.schema.to_owned()
    }

    /// Get the document id of this log.
    pub fn document(&self) -> Hash {
        self.document.to_owned()
    }

    /// Add an entry to this log.
    pub fn add_entry(&mut self, entry: LogEntry) {
        self.entries.push(entry)
    }
}
