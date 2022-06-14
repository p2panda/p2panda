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
pub trait OperationStore<StorageOperation: AsVerifiedOperation> {
    /// Insert an operation into the db.
    ///
    /// The passed operation must implement the `AsVerifiedOperation` trait. Errors when
    /// a fatal DB error occurs.
    async fn insert_operation(
        &self,
        operation: &StorageOperation,
        document_id: &DocumentId,
    ) -> Result<(), OperationStorageError>;

    /// Get an operation identified by it's OperationId.
    ///
    /// Returns a type implementing `AsVerifiedOperation` which includes `Author`, `DocumentId` and
    /// `OperationId` metadata.
    async fn get_operation_by_id(
        &self,
        id: &OperationId,
    ) -> Result<Option<StorageOperation>, OperationStorageError>;

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
    ) -> Result<Vec<StorageOperation>, OperationStorageError>;
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use rstest::rstest;
    use std::convert::TryFrom;

    use crate::document::DocumentId;
    use crate::entry::LogId;
    use crate::identity::{Author, KeyPair};
    use crate::operation::{
        AsOperation, AsVerifiedOperation, Operation, OperationId, VerifiedOperation,
    };
    use crate::storage_provider::errors::OperationStorageError;
    use crate::storage_provider::traits::test_utils::{
        aquadoggo_test_db, SimplestStorageProvider, TestStore,
    };
    use crate::storage_provider::traits::{AsStorageEntry, EntryStore, StorageProvider};
    use crate::test_utils::constants::{default_fields, DEFAULT_HASH};
    use crate::test_utils::fixtures::{
        create_operation, delete_operation, document_id, key_pair, operation_fields, operation_id,
        public_key, random_previous_operations, update_operation, verified_operation,
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
            let operations = self.operations.lock().unwrap();
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
            let operations = self.operations.lock().unwrap();
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
            let operations = self.operations.lock().unwrap();
            Ok(operations
                .iter()
                .filter(|(document_id, _verified_operation)| document_id == id)
                .map(|(_, operation)| operation.clone())
                .collect())
        }
    }

    #[rstest]
    #[case::create_operation(create_operation(&default_fields()))]
    #[case::update_operation(update_operation(&default_fields(), &DEFAULT_HASH.parse().unwrap()))]
    #[case::update_operation_many_prev_ops(update_operation(&default_fields(), &random_previous_operations(12)))]
    #[case::delete_operation(delete_operation(&DEFAULT_HASH.parse().unwrap()))]
    #[case::delete_operation_many_prev_ops(delete_operation(&random_previous_operations(12)))]
    #[async_std::test]
    async fn insert_get_operations(
        #[case] operation: Operation,
        #[from(public_key)] author: Author,
        operation_id: OperationId,
        document_id: DocumentId,
        #[from(aquadoggo_test_db)]
        #[future]
        db: TestStore,
    ) {
        let db = db.await;
        // Construct the storage operation.
        let operation = VerifiedOperation::new(&author, &operation_id, &operation).unwrap();

        // Insert the doggo operation into the db, returns Ok(true) when succesful.
        let result = db.store.insert_operation(&operation, &document_id).await;
        assert!(result.is_ok());

        // Request the previously inserted operation by it's id.
        let returned_operation = db
            .store
            .get_operation_by_id(operation.operation_id())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(returned_operation.public_key(), operation.public_key());
        assert_eq!(returned_operation.fields(), operation.fields());
        assert_eq!(returned_operation.operation_id(), operation.operation_id());
    }

    #[rstest]
    #[async_std::test]
    async fn insert_operation_twice(
        #[from(verified_operation)] verified_operation: VerifiedOperation,
        document_id: DocumentId,
        #[from(aquadoggo_test_db)]
        #[future]
        db: TestStore,
    ) {
        let db = db.await;

        assert!(db
            .store
            .insert_operation(&verified_operation, &document_id)
            .await
            .is_ok());

        assert_eq!(
            db.store.insert_operation(&verified_operation, &document_id).await.unwrap_err().to_string(),
            format!("Error occured when inserting an operation with id OperationId(Hash(\"{}\")) into storage", verified_operation.operation_id().as_str())
        )
    }

    #[rstest]
    #[async_std::test]
    async fn gets_document_by_operation_id(
        #[from(verified_operation)]
        #[with(Some(operation_fields(default_fields())), None, None, None, Some(DEFAULT_HASH.parse().unwrap()))]
        create_operation: VerifiedOperation,
        #[from(verified_operation)]
        #[with(Some(operation_fields(default_fields())), Some(DEFAULT_HASH.parse().unwrap()))]
        update_operation: VerifiedOperation,
        document_id: DocumentId,
        #[from(aquadoggo_test_db)]
        #[future]
        db: TestStore,
    ) {
        let db = db.await;

        assert!(db
            .store
            .get_document_by_operation_id(create_operation.operation_id())
            .await
            .unwrap()
            .is_none());

        db.store
            .insert_operation(&create_operation, &document_id)
            .await
            .unwrap();

        assert_eq!(
            db.store
                .get_document_by_operation_id(create_operation.operation_id())
                .await
                .unwrap()
                .unwrap(),
            document_id.clone()
        );

        db.store
            .insert_operation(&update_operation, &document_id)
            .await
            .unwrap();

        assert_eq!(
            db.store
                .get_document_by_operation_id(create_operation.operation_id())
                .await
                .unwrap()
                .unwrap(),
            document_id.clone()
        );
    }

    #[rstest]
    #[async_std::test]
    async fn get_operations_by_document_id(
        key_pair: KeyPair,
        #[from(aquadoggo_test_db)]
        #[with(5, 1)]
        #[future]
        db: TestStore,
    ) {
        let db = db.await;
        let author = Author::try_from(key_pair.public_key().to_owned()).unwrap();

        let latest_entry = db
            .store
            .get_latest_entry(&author, &LogId::default())
            .await
            .unwrap()
            .unwrap();

        let document_id = db
            .store
            .get_document_by_entry(&latest_entry.hash())
            .await
            .unwrap()
            .unwrap();

        let operations_by_document_id = db
            .store
            .get_operations_by_document_id(&document_id)
            .await
            .unwrap();

        assert_eq!(operations_by_document_id.len(), 5)
    }
}
