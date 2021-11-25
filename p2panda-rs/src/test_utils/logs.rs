// SPDX-License-Identifier: AGPL-3.0-or-later

//! Structs which encapsulate log data structures and logic.
//! 
//! Much of the logic here looks different from how it will in a  real world p2panda application. As more 
//! functionality is implemented in the main library this will be replaced with core modules. For these reasons 
//! this code is only intended for testing or demo purposes.

use crate::entry::EntrySigned;
use crate::hash::Hash;
use crate::identity::Author;
use crate::message::Message;

/// This struct is an augmented version of a simple log entry. It includes extra properties to aid in 
/// testing and materialising instances. In particular it has an `previous_operation`.
/// which our panda messages currently don't have and will need in the future.
#[derive(Clone, Debug)]
pub struct LogEntry {
    /// The author of this entry
    pub author: Author,
    /// The author of the instance this entry is part of
    pub instance_author: Option<String>,
    /// The encoded entry
    pub entry_encoded: EntrySigned,
    /// The message
    pub message: Message,
    /// The hash of the entry this message/operation acts upon
    pub previous_operation: Option<String>,
}

/// Struct to represent a bamboo log
#[derive(Clone, Debug)]
pub struct Log {
    /// The id of this log
    pub id: i64,
    /// The schema of this log
    pub schema: String,
    /// The entries in this log
    pub entries: Vec<LogEntry>,
}

impl LogEntry {
    /// Create a new log
    pub fn new(
        author: Author,
        instance_author: Option<String>,
        entry_encoded: EntrySigned,
        message: Message,
        previous_operation: Option<String>,
    ) -> Self {
        Self {
            author,
            instance_author,
            entry_encoded,
            message,
            previous_operation,
        }
    }

    /// Get the author of this entry
    pub fn author(&self) -> String {
        self.author.as_str().to_string().clone()
    }

    /// Get the hash of this entry
    pub fn hash(&self) -> Hash {
        self.entry_encoded.hash().clone()
    }

    /// Get the hash of this entry as a string
    pub fn hash_str(&self) -> String {
        self.entry_encoded.hash().as_str().to_string().clone()
    }

    /// Get the author of the instance this entry belongs to
    pub fn instance_author(&self) -> String {
        self.instance_author.clone().unwrap().as_str().to_string()
    }

    /// Get the message from this entry
    pub fn message(&self) -> Message {
        self.message.clone()
    }

    /// Get the instance backlink for this entry
    pub fn previous_operation(&self) -> Option<String> {
        self.previous_operation.to_owned().clone()
    }
}

impl Log {
    /// Create a new log
    pub fn new(log_id: i64, schema: String) -> Self {
        Self {
            id: log_id,
            schema: schema.into(),
            entries: Vec::new(),
        }
    }

    /// Get the entries from this log
    pub fn entries(&self) -> Vec<LogEntry> {
        self.entries.to_owned()
    }

    /// Get the id of this log
    pub fn id(&self) -> i64 {
        self.id.to_owned()
    }

    /// Get the schema of this log
    pub fn schema(&self) -> String {
        self.schema.to_owned()
    }

    /// Add an entry to this log
    pub fn add_entry(&mut self, entry: LogEntry) {
        self.entries.push(entry)
    }
}
