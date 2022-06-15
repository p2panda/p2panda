// SPDX-License-Identifier: AGPL-3.0-or-later

//! Structs which encapsulate log data structures and logic.
//!
//! Much of the logic here looks different from how it will in a real world p2panda application. As
//! more functionality is implemented in the main library this will be replaced with core modules.
//! For these reasons this code is only intended for testing or demo purposes.
use std::convert::TryFrom;
use std::slice::Iter;

use crate::document::DocumentViewId;
use crate::entry::{decode_entry, EntrySigned, LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::Author;
use crate::operation::{AsOperation, Operation, OperationEncoded};
use crate::schema::{Schema, SchemaId};

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
    pub fn new(entry_encoded: &EntrySigned, operation_encoded: &OperationEncoded) -> Self {
        Self {
            entry_encoded: entry_encoded.to_owned(),
            operation_encoded: operation_encoded.to_owned(),
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
    pub fn operation(&self, schema: &Schema) -> Operation {
        self.operation_encoded.decode(schema).unwrap()
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
    pub fn previous_operations(&self, schema: &Schema) -> Option<DocumentViewId> {
        self.operation(schema).previous_operations()
    }
}

/// An append only log containing entries for the same document and author.
#[derive(Debug, Clone)]
pub struct Log {
    /// Public key of the author.
    author: Author,

    /// Log id used for this document.
    log_id: LogId,

    /// Hash that identifies the document this log is for.
    document: Hash,

    /// Schema schema for this log.
    schema: SchemaId,

    /// The entries in this log.
    entries: Vec<LogEntry>,
}

impl Log {
    /// Create a new log.
    pub fn new(
        document_id: Hash,
        entry_signed: &EntrySigned,
        operation_encoded: &OperationEncoded,
        schema: &Schema,
    ) -> Self {
        let entry = decode_entry(entry_signed, Some(operation_encoded), Some(schema)).unwrap();
        let mut log = Self {
            author: entry_signed.author(),
            log_id: entry.log_id().to_owned(),
            document: document_id,
            schema: entry.operation().unwrap().schema(),
            entries: Vec::new(),
        };
        log.add_entry(LogEntry::new(entry_signed, operation_encoded));
        log
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
    pub fn schema(&self) -> SchemaId {
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

    /// Returns the next sequence number for this log.
    pub fn next_seq_num(&self) -> SeqNum {
        SeqNum::new((self.entries.len() + 1) as u64).unwrap()
    }
}

/// All the logs an author has on this node.
#[derive(Clone, Debug)]
pub struct AuthorLogs(Vec<Log>);

impl AuthorLogs {
    /// Create a new empty collection of author logs.
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// Create a new log for this author and insert into collection.
    pub fn create_new_log(
        &mut self,
        document_id: Hash,
        entry_signed: &EntrySigned,
        operation_encoded: &OperationEncoded,
        schema: &Schema,
    ) {
        self.0.push(Log::new(
            document_id,
            entry_signed,
            operation_encoded,
            schema,
        ))
    }

    /// Returns the number of logs this author owns.
    pub fn len(&self) -> usize {
        self.0.len()
    }
    /// Returns the number of logs this author owns.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns an iterator over all logs by this author.
    pub fn iter(&self) -> Iter<Log> {
        self.0.iter()
    }

    /// Get a full log by it's document id.
    pub fn get_log_by_document_id(&self, document_id: &Hash) -> Option<&Log> {
        self.0.iter().find(|log| log.document() == *document_id)
    }

    /// Get the next available log id for this author.
    pub fn next_log_id(&self) -> LogId {
        LogId::new((self.0.len() + 1) as u64)
    }

    /// Find the log id for the given document.
    pub fn get_document_log_id(&self, document_id: &Hash) -> LogId {
        let document_log = self.iter().find(|log| log.document() == *document_id);
        match document_log {
            Some(log) => log.id(),
            None => self.next_log_id(),
        }
    }

    /// Find a document log which contains the passed entry.
    pub fn find_document_log_by_entry(&self, entry: &Hash) -> Option<&Log> {
        self.0.iter().find(|log| {
            log.entries()
                .iter()
                .any(|log_entry| log_entry.hash() == *entry)
        })
    }

    /// Get a mutable reference to a log by this author identified by it's log id.
    pub fn get_log_mut(&mut self, id: &LogId) -> Option<&mut Log> {
        self.0.iter_mut().find(|log| log.id() == *id)
    }
}

impl Default for AuthorLogs {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::entry::{decode_entry, EntrySigned, LogId};
    use crate::operation::{AsOperation, OperationEncoded};
    use crate::schema::Schema;
    use crate::test_utils::fixtures::{default_schema, entry_signed_encoded, operation_encoded};

    use super::{AuthorLogs, Log, LogEntry};

    #[rstest]
    fn log_entry(
        entry_signed_encoded: EntrySigned,
        operation_encoded: OperationEncoded,
        default_schema: Schema,
    ) {
        let entry = decode_entry(
            &entry_signed_encoded,
            Some(&operation_encoded),
            Some(&default_schema),
        )
        .unwrap();
        let log_entry = LogEntry::new(&entry_signed_encoded, &operation_encoded);

        assert_eq!(log_entry.entry_encoded(), entry_signed_encoded);
        assert_eq!(
            log_entry.operation_encoded().hash(),
            operation_encoded.hash()
        );
        assert_eq!(log_entry.author(), entry_signed_encoded.author().as_str());
        assert_eq!(log_entry.hash(), entry_signed_encoded.hash());
        assert_eq!(
            log_entry.operation(&default_schema).schema(),
            entry.operation().unwrap().schema()
        );
    }

    #[rstest]
    fn log(
        entry_signed_encoded: EntrySigned,
        operation_encoded: OperationEncoded,
        default_schema: Schema,
    ) {
        let entry = decode_entry(
            &entry_signed_encoded,
            Some(&operation_encoded),
            Some(&default_schema),
        )
        .unwrap();
        let log = Log::new(
            entry_signed_encoded.hash(),
            &entry_signed_encoded,
            &operation_encoded,
            &default_schema,
        );

        assert_eq!(log.entries().len(), 1);
        assert_eq!(log.next_seq_num().as_u64(), 2);
        assert_eq!(log.author(), entry_signed_encoded.author());
        assert_eq!(log.document(), entry_signed_encoded.hash());
        assert_eq!(log.schema(), entry.operation().unwrap().schema());
    }

    #[rstest]
    fn author_logs(
        entry_signed_encoded: EntrySigned,
        operation_encoded: OperationEncoded,
        default_schema: Schema,
    ) {
        let mut author_logs = AuthorLogs::new();
        author_logs.create_new_log(
            entry_signed_encoded.hash(),
            &entry_signed_encoded,
            &operation_encoded,
            &default_schema,
        );

        assert_eq!(author_logs.len(), 1);
        assert_eq!(
            author_logs
                .get_log_by_document_id(&entry_signed_encoded.hash())
                .unwrap()
                .id(),
            LogId::new(1)
        );
        assert_eq!(author_logs.next_log_id(), LogId::new(2));
        assert_eq!(
            author_logs.get_document_log_id(&entry_signed_encoded.hash()),
            LogId::new(1)
        );
        assert_eq!(
            author_logs
                .find_document_log_by_entry(&entry_signed_encoded.hash())
                .unwrap()
                .id(),
            LogId::new(1)
        );
    }
}
