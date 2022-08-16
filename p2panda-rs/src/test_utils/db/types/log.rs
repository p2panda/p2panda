// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::document::DocumentId;
use crate::entry::LogId;
use crate::identity::Author;
use crate::schema::SchemaId;
use crate::storage_provider::traits::AsStorageLog;

/// A log entry represented as a concatenated string of `"{author}-{schema}-{document_id}-{log_id}"`
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct StorageLog {
    /// Public key of the author.
    pub author: Author,

    /// Log id used for this document.
    pub log_id: LogId,

    /// Hash that identifies the document this log is for.
    pub document: DocumentId,

    /// SchemaId which identifies the schema for operations in this log.
    pub schema: SchemaId,
}

/// Implement `AsStorageLog` trait for our `StorageLog` struct
impl AsStorageLog for StorageLog {
    fn new(author: &Author, schema: &SchemaId, document: &DocumentId, log_id: &LogId) -> Self {
        StorageLog {
            author: author.clone(),
            log_id: *log_id,
            document: document.clone(),
            schema: schema.clone(),
        }
    }

    fn author(&self) -> Author {
        self.author.clone()
    }

    fn schema_id(&self) -> SchemaId {
        self.schema.clone()
    }

    fn document_id(&self) -> DocumentId {
        self.document.clone()
    }

    fn id(&self) -> LogId {
        self.log_id
    }
}
