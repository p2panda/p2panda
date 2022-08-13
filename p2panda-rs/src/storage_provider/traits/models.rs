// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::document::DocumentId;
use crate::entry::traits::{AsEncodedEntry, AsEntry};
use crate::entry::LogId;
use crate::identity::Author;
use crate::operation::EncodedOperation;
use crate::schema::SchemaId;

/// Trait to be implemented on a struct representing a stored entry optionally with it's payload.
///
/// Storage implementations should implement this for a data structure that represents an
/// entry as it is stored in the database. This trait requires implementations of both `AsEntry`
/// and `AsEncodedEntry` and additionally adds a method for accessing the entries'  payload.
pub trait EntryWithOperation: AsEntry + AsEncodedEntry {
    /// The payload of this operation.
    fn payload(&self) -> Option<&EncodedOperation>;
}

/// Trait to be implemented on a struct representing a stored log.
///
/// Storage implementations should implement this for a data structure that represents a
/// log as it is stored in the database. This trait defines methods for reading values from the
/// log.
pub trait AsStorageLog: Sized + Send + Sync {
    /// Constructor method for structs.
    fn new(author: &Author, schema: &SchemaId, document: &DocumentId, log_id: &LogId) -> Self;

    /// Returns the LogId of this log.
    fn id(&self) -> LogId;

    /// Returns the Author of this log.
    fn author(&self) -> Author;

    /// Returns the DocumentId of this log.
    fn document_id(&self) -> DocumentId;

    /// Returns the SchemaId of this log.
    fn schema_id(&self) -> SchemaId;
}
