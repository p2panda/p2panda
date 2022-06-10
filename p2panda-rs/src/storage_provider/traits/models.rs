// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::document::{DocumentId, DocumentViewId};
use crate::entry::{EntrySigned, LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::Author;
use crate::operation::{
    Operation, OperationAction, OperationEncoded, OperationFields, OperationId,
};
use crate::schema::SchemaId;
use crate::Validate;

/// Trait to be implemented on a struct representing a stored entry.
///
/// Storage implementations should implement this for a data structure that represents an
/// entry as it is stored in the database. This trait defines methods for reading values from the
/// entry and it's operation.
pub trait AsStorageEntry:
    Sized + Clone + Send + Sync + Validate + PartialEq + std::fmt::Debug
{
    /// The error type returned by this traits' methods.
    type AsStorageEntryError: 'static + std::error::Error + Send + Sync;

    /// Construct an instance of the struct implementing `AsStorageEntry`
    fn new(
        entry: &EntrySigned,
        operation: &OperationEncoded,
    ) -> Result<Self, Self::AsStorageEntryError>;

    /// Returns the author of this entry.
    fn author(&self) -> Author;

    /// Returns the hash of this entry.
    fn hash(&self) -> Hash;

    /// Returns the bytes of the signed encoded entry.
    fn entry_bytes(&self) -> Vec<u8>;

    /// Returns hash of backlink entry when given.
    fn backlink_hash(&self) -> Option<Hash>;

    /// Returns hash of skiplink entry when given.
    fn skiplink_hash(&self) -> Option<Hash>;

    /// Returns the sequence number of this entry.
    fn seq_num(&self) -> SeqNum;

    /// Returns the log id of this entry.
    fn log_id(&self) -> LogId;

    /// Returns the operation contained on this entry.
    fn operation(&self) -> Operation;
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

// /// Trait to be implemented on a struct representing a stored operation.
// ///
// /// Contains the values of an operation as well as it's author, it's id, and the id of
// /// it's document.
// ///
// /// Storage implementations should implement this for a data structure that represents an
// /// operation as it is stored in the database. This trait defines methods for reading
// /// values from the operation and some meta data.
// pub trait AsStorageOperation: Sized + Clone + Send + Sync {
//     /// The error type returned by this traits' methods.
//     type AsStorageOperationError: 'static + std::error::Error;

//     /// The action this operation performs.
//     fn action(&self) -> OperationAction;

//     /// The author who published this operation.
//     fn author(&self) -> Author;

//     /// The id of the document this operation is part of.
//     fn document_id(&self) -> DocumentId;

//     /// The fields contained in this operation.
//     fn fields(&self) -> Option<OperationFields>;

//     /// The id of this operation.
//     fn id(&self) -> OperationId;

//     /// The previous operations for this operation.
//     fn previous_operations(&self) -> Option<DocumentViewId>;

//     /// The id of the schema this operation follows.
//     fn schema_id(&self) -> SchemaId;
// }
