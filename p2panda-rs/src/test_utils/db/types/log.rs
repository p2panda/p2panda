// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::document::DocumentId;
use crate::entry::LogId;
use crate::identity::PublicKey;
use crate::schema::SchemaId;
use crate::storage_provider::traits::AsStorageLog;

/// A log entry represented as a concatenated string of `"{public_key}-{schema}-{document_id}-{log_id}"`
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct StorageLog {
    /// Public key of the author.
    pub public_key: PublicKey,

    /// Log id used for this document.
    pub log_id: LogId,

    /// Hash that identifies the document this log is for.
    pub document: DocumentId,

    /// SchemaId which identifies the schema for operations in this log.
    pub schema: SchemaId,
}

/// Implement `AsStorageLog` trait for our `StorageLog` struct
impl AsStorageLog for StorageLog {
    fn new(
        public_key: &PublicKey,
        schema: &SchemaId,
        document: &DocumentId,
        log_id: &LogId,
    ) -> Self {
        StorageLog {
            public_key: *public_key,
            log_id: *log_id,
            document: document.clone(),
            schema: schema.clone(),
        }
    }

    fn public_key(&self) -> PublicKey {
        self.public_key
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
