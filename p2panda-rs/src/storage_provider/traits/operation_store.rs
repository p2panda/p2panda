// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;

use crate::document::DocumentId;
use crate::identity::PublicKey;
use crate::operation::traits::{AsOperation, WithPublicKey};
use crate::operation::{Operation, OperationId};
use crate::schema::SchemaId;
use crate::storage_provider::error::OperationStorageError;
use crate::WithId;

/// Trait which handles all storage actions relating to `Operation`s.
///
/// This trait should be implemented on the root storage provider struct. It's definitions make up
/// the required methods for inserting and querying operations from storage.
#[async_trait]
pub trait OperationStore {
    /// An associated type representing an operation as it passes in and out of storage.
    type Operation: AsOperation + WithId<OperationId> + WithId<DocumentId> + WithPublicKey + Sync;

    /// Insert an operation into the db.
    ///
    /// The passed operation must implement the `AsVerifiedOperation` trait. Errors when
    /// a fatal DB error occurs.
    async fn insert_operation(
        &self,
        id: &OperationId,
        public_key: &PublicKey,
        operation: &Operation,
        document_id: &DocumentId,
    ) -> Result<(), OperationStorageError>;

    /// Get an operation identified by it's OperationId.
    ///
    /// Returns a type implementing `AsVerifiedOperation` which includes `PublicKey`, `DocumentId` and
    /// `OperationId` metadata.
    async fn get_operation(
        &self,
        id: &OperationId,
    ) -> Result<Option<Self::Operation>, OperationStorageError>;

    /// Get the id of the document an operation is contained within.
    ///
    /// If no document was found, then this method returns a result wrapping
    /// a None variant.
    async fn get_document_id_by_operation_id(
        &self,
        id: &OperationId,
    ) -> Result<Option<DocumentId>, OperationStorageError>;

    /// Get all operations which are part of a specific document.
    ///
    /// Returns a result containing a vector of operations. If no document
    /// was found then an empty vector is returned. Errors if a fatal storage
    /// error occured.
    async fn get_operations_by_document_id(
        &self,
        id: &DocumentId,
    ) -> Result<Vec<Self::Operation>, OperationStorageError>;

    /// Get all operations which follow a certain schema.
    ///
    /// Returns a result containing a vector of operations. If no schema
    /// was found then an empty vector is returned. Errors if a fatal storage
    /// error occured.
    async fn get_operations_by_schema_id(
        &self,
        id: &SchemaId,
    ) -> Result<Vec<Self::Operation>, OperationStorageError>;
}
