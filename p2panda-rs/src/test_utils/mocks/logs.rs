// SPDX-License-Identifier: AGPL-3.0-or-later

//! Structs which encapsulate log data structures and logic.
//!
//! Much of the logic here looks different from how it will in a  real world p2panda application.
//! As more functionality is implemented in the main library this will be replaced with core
//! modules. For these reasons this code is only intended for testing or demo purposes.
use std::convert::TryFrom;

use crate::entry::EntrySigned;
use crate::hash::Hash;
use crate::identity::Author;
use crate::operation::{Operation, OperationEncoded};

/// This struct is an augmented version of a simple log entry. It includes extra properties to aid
/// in testing and materialising instances.
#[derive(Clone, Debug)]
pub struct LogEntry {
    /// The author of this entry.
    pub author: Author,

    /// The author of the instance this entry is part of.
    pub instance_author: Option<String>,

    /// The encoded entry.
    pub entry_encoded: EntrySigned,

    /// The operation.
    pub operation: Operation,

    /// The hash of the entry this operation acts upon.
    pub previous_operation: Option<String>,
}

/// Struct to represent a bamboo log.
#[derive(Clone, Debug)]
pub struct Log {
    /// The id of this log.
    pub id: i64,

    /// The schema of this log.
    pub schema: String,

    /// The document id of this log.
    pub document_id: String,

    /// The entries in this log.
    pub entries: Vec<LogEntry>,
}

impl LogEntry {
    /// Create a new log.
    pub fn new(
        author: Author,
        instance_author: Option<String>,
        entry_encoded: EntrySigned,
        operation: Operation,
        previous_operation: Option<String>,
    ) -> Self {
        Self {
            author,
            instance_author,
            entry_encoded,
            operation,
            previous_operation,
        }
    }

    /// Get the author of this entry.
    pub fn author(&self) -> String {
        self.author.as_str().to_string()
    }

    /// Get the hash of this entry.
    pub fn hash(&self) -> Hash {
        self.entry_encoded.hash()
    }

    /// Get the hash of this entry as a string.
    pub fn hash_str(&self) -> String {
        self.entry_encoded.hash().as_str().to_string()
    }

    /// Get the author of the instance this entry belongs to.
    pub fn instance_author(&self) -> String {
        self.instance_author.clone().unwrap().as_str().to_string()
    }

    /// Get the operation from this entry.
    pub fn operation(&self) -> Operation {
        self.operation.clone()
    }

    /// Get the encoded entry from this entry.
    pub fn entry_encoded(&self) -> EntrySigned {
        self.entry_encoded.clone()
    }

    /// Get the encoded operation from this entry.
    pub fn operation_encoded(&self) -> OperationEncoded {
        OperationEncoded::try_from(&self.operation).unwrap()
    }

    /// Get the previous operation hash for this entry.
    pub fn previous_operation(&self) -> Option<String> {
        self.previous_operation.to_owned()
    }
}

impl Log {
    /// Create a new log.
    pub fn new(log_id: i64, schema: String, document_id: String) -> Self {
        Self {
            id: log_id,
            schema,
            document_id,
            entries: Vec::new(),
        }
    }

    /// Get the entries from this log.
    pub fn entries(&self) -> Vec<LogEntry> {
        self.entries.to_owned()
    }

    /// Get the id of this log.
    pub fn id(&self) -> i64 {
        self.id.to_owned()
    }

    /// Get the schema of this log.
    pub fn schema(&self) -> String {
        self.schema.to_owned()
    }

    /// Get the document id of this log.
    pub fn document(&self) -> String {
        self.document_id.to_owned()
    }

    /// Add an entry to this log.
    pub fn add_entry(&mut self, entry: LogEntry) {
        self.entries.push(entry)
    }
}
