// SPDX-License-Identifier: AGPL-3.0-or-later

//! Data models which are used in StorageProvider.

use std::fmt::Debug;

use super::StorageProviderError;
use crate::document::DocumentId;
use crate::entry::{decode_entry, EntrySigned, LogId};
use crate::identity::Author;
use crate::operation::OperationEncoded;
use crate::schema::SchemaId;
use crate::Validate;

/// Struct wrapping an entry with it's operation.
///
/// Used internally throughout `storage_provider` in method args and default trait definitions.
/// The `AsStorageEntry` trait requires `TryFrom<EntryWithOperation>` & `TryInto<EntryWithOperation>`
/// conversion traits to be present.
#[derive(Debug, Clone)]
pub struct EntryWithOperation(EntrySigned, OperationEncoded);

impl EntryWithOperation {
    /// Instantiate a new EntryWithOperation.
    pub fn new(
        entry: EntrySigned,
        operation: OperationEncoded,
    ) -> Result<Self, StorageProviderError> {
        // TODO: Validate entry + operation here
        let entry_with_operation = Self(entry, operation);
        entry_with_operation.validate()?;
        Ok(entry_with_operation)
    }

    /// Returns a reference to the encoded entry.
    pub fn entry_encoded(&self) -> &EntrySigned {
        &self.0
    }

    /// Returns a refernce to the optional encoded operation.
    pub fn operation_encoded(&self) -> &OperationEncoded {
        &self.1
    }
}

impl Validate for EntryWithOperation {
    type Error = StorageProviderError;

    fn validate(&self) -> Result<(), Self::Error> {
        self.entry_encoded().validate()?;
        self.operation_encoded().validate()?;
        decode_entry(self.entry_encoded(), Some(self.operation_encoded()))?;
        Ok(())
    }
}

/// Struct representing a bamboo append-only log structure,
#[derive(Debug, Clone)]

pub struct Log {
    /// Public key of the author.
    pub author: Author,

    /// Log id used for this document.
    pub log_id: LogId,

    /// Hash that identifies the document this log is for.
    pub document: DocumentId,

    /// SchemaId which identifies the schema for operations in this log.
    pub schema: SchemaId,
}

impl Log {
    pub fn new(author: Author, schema: SchemaId, document: DocumentId, log_id: LogId) -> Log {
        Log {
            author,
            log_id,
            document,
            schema,
        }
    }

    pub fn author(&self) -> &Author {
        &self.author
    }

    pub fn log_id(&self) -> &LogId {
        &self.log_id
    }

    pub fn document(&self) -> &DocumentId {
        &self.document
    }

    pub fn schema(&self) -> &SchemaId {
        &self.schema
    }
}
