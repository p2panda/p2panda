// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;

use crate::document::DocumentId;
use crate::operation::OperationId;
use crate::storage_provider::errors::OperationStorageError;
use crate::storage_provider::traits::AsStorageOperation;

/// Trait which handles all storage actions relating to `Operation`s.
///
/// This trait should be implemented on the root storage provider struct. It's definitions make up
/// the required methods for inserting and querying operations from storage.
#[async_trait]
pub trait OperationStore<StorageOperation: AsStorageOperation> {
    /// Insert an operation into the db.
    ///
    /// The passed operation must implement the `AsStorageOperation` trait. Errors when
    /// a fatal DB error occurs.
    async fn insert_operation(
        &self,
        operation: &StorageOperation,
    ) -> Result<(), OperationStorageError>;

    /// Get an operation identified by it's OperationId.
    ///
    /// Returns a type implementing `AsStorageOperation` which includes `Author`, `DocumentId` and
    /// `OperationId` metadata.
    async fn get_operation_by_id(
        &self,
        id: OperationId,
    ) -> Result<Option<StorageOperation>, OperationStorageError>;

    /// Get the id of the document an operation is contained within.
    ///
    /// If no document was found, then this method returns a result wrapping
    /// a None variant.
    async fn get_document_by_operation_id(
        &self,
        id: OperationId,
    ) -> Result<Option<DocumentId>, OperationStorageError>;

    /// Get all operations which are part of a specific document.
    ///
    /// Returns a result containing a vector of operations. If no document
    /// was found then an empty vector is returned. Errors if a fatal storage
    /// error occured.
    async fn get_operations_by_document_id(
        &self,
        id: &DocumentId,
    ) -> Result<Vec<StorageOperation>, OperationStorageError>;
}
