// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;

use crate::document::DocumentId;
use crate::operation::AsVerifiedOperation;
use crate::operation::OperationId;
use crate::storage_provider::errors::OperationStorageError;

/// Trait which handles all storage actions relating to `Operation`s.
///
/// This trait should be implemented on the root storage provider struct. It's definitions make up
/// the required methods for inserting and querying operations from storage.
#[async_trait]
pub trait OperationStore<Operation: AsVerifiedOperation> {
    /// Insert an operation into the db.
    ///
    /// The passed operation must implement the `AsVerifiedOperation` trait. Errors when
    /// a fatal DB error occurs.
    async fn insert_operation(
        &self,
        operation: &Operation,
        document_id: &DocumentId,
    ) -> Result<(), OperationStorageError>;

    /// Get an operation identified by it's OperationId.
    ///
    /// Returns a type implementing `AsVerifiedOperation` which includes `Author`, `DocumentId` and
    /// `OperationId` metadata.
    async fn get_operation_by_id(
        &self,
        id: &OperationId,
    ) -> Result<Option<Operation>, OperationStorageError>;

    /// Get the id of the document an operation is contained within.
    ///
    /// If no document was found, then this method returns a result wrapping
    /// a None variant.
    async fn get_document_by_operation_id(
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
    ) -> Result<Vec<Operation>, OperationStorageError>;
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use crate::{
        document::DocumentId,
        operation::{AsVerifiedOperation, OperationId, VerifiedOperation},
        storage_provider::{
            errors::OperationStorageError, traits::test_utils::SimplestStorageProvider,
        },
    };

    use super::OperationStore;

    #[async_trait]
    impl OperationStore<VerifiedOperation> for SimplestStorageProvider {
        async fn insert_operation(
            &self,
            operation: &VerifiedOperation,
            document_id: &DocumentId,
        ) -> Result<(), OperationStorageError> {
            let mut operations = self.operations.lock().unwrap();
            if operations
                .iter()
                .any(|(_document_id, verified_operation)| verified_operation == operation)
            {
                return Err(OperationStorageError::InsertionError(
                    operation.operation_id().clone(),
                ));
            } else {
                operations.push((document_id.clone(), operation.clone()))
            };
            Ok(())
        }

        /// Get an operation identified by it's OperationId.
        ///
        /// Returns a type implementing `AsVerifiedOperation` which includes `Author`, `DocumentId` and
        /// `OperationId` metadata.
        async fn get_operation_by_id(
            &self,
            id: &OperationId,
        ) -> Result<Option<VerifiedOperation>, OperationStorageError> {
            let mut operations = self.operations.lock().unwrap();
            Ok(operations
                .iter()
                .find(|(_document_id, verified_operation)| verified_operation.operation_id() == id)
                .map(|(_, operation)| operation.clone()))
        }

        /// Get the id of the document an operation is contained within.
        ///
        /// If no document was found, then this method returns a result wrapping
        /// a None variant.
        async fn get_document_by_operation_id(
            &self,
            id: &OperationId,
        ) -> Result<Option<DocumentId>, OperationStorageError> {
            let mut operations = self.operations.lock().unwrap();
            Ok(operations
                .iter()
                .find(|(_document_id, verified_operation)| verified_operation.operation_id() == id)
                .map(|(document_id, _operation)| document_id.clone()))
        }

        /// Get all operations which are part of a specific document.
        ///
        /// Returns a result containing a vector of operations. If no document
        /// was found then an empty vector is returned. Errors if a fatal storage
        /// error occured.
        async fn get_operations_by_document_id(
            &self,
            id: &DocumentId,
        ) -> Result<Vec<VerifiedOperation>, OperationStorageError> {
            let mut operations = self.operations.lock().unwrap();
            Ok(operations
                .iter()
                .filter(|(document_id, _verified_operation)| document_id == id)
                .map(|(_, operation)| operation.clone())
                .collect())
        }
    }
}
