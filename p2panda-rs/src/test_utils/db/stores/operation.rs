// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;
use log::debug;

use crate::document::DocumentId;
use crate::identity::PublicKey;
use crate::operation::traits::{AsOperation, WithOperationID};
use crate::operation::{Operation, OperationId};
use crate::storage_provider::error::OperationStorageError;
use crate::storage_provider::traits::OperationStore;
use crate::test_utils::db::{MemoryStore, PublishedOperation};

#[async_trait]
impl OperationStore<PublishedOperation> for MemoryStore {
    async fn insert_operation(
        &self,
        id: &OperationId,
        public_key: &PublicKey,
        operation: &Operation,
        document_id: &DocumentId,
    ) -> Result<(), OperationStorageError> {
        debug!(
            "Inserting {} operation: {} into store",
            operation.action().as_str(),
            id,
        );

        let mut operations = self.operations.lock().unwrap();

        let is_duplicate_id = operations
            .values()
            .any(|(_document_id, verified_operation)| verified_operation.0 == *id);

        if is_duplicate_id {
            return Err(OperationStorageError::InsertionError(id.clone()));
        }

        let operation = PublishedOperation(id.clone(), operation.clone(), *public_key);
        operations.insert(id.clone(), (document_id.clone(), operation));

        Ok(())
    }

    /// Get an operation identified by it's OperationId.
    ///
    /// Returns a type implementing `AsVerifiedOperation` which includes `PublicKey`, `DocumentId` and
    /// `OperationId` metadata.
    async fn get_operation_by_id(
        &self,
        id: &OperationId,
    ) -> Result<Option<PublishedOperation>, OperationStorageError> {
        let operations = self.operations.lock().unwrap();
        Ok(operations.get(id).map(|(_, operation)| operation.clone()))
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
            .values()
            .find(|(_document_id, verified_operation)| verified_operation.id() == id)
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
    ) -> Result<Vec<PublishedOperation>, OperationStorageError> {
        let operations = self.operations.lock().unwrap();
        Ok(operations
            .values()
            .filter(|(document_id, _verified_operation)| document_id == id)
            .map(|(_, operation)| operation.clone())
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::document::DocumentId;
    use crate::entry::traits::AsEncodedEntry;
    use crate::entry::LogId;
    use crate::identity::{KeyPair, PublicKey};
    use crate::operation::traits::{AsOperation, WithOperationID, WithPublicKey};
    use crate::operation::{Operation, OperationId};
    use crate::storage_provider::traits::{EntryStore, StorageProvider};
    use crate::test_utils::constants;
    use crate::test_utils::db::test_db::{test_db, TestDatabase};
    use crate::test_utils::fixtures::{
        create_operation, delete_operation, document_id, key_pair, operation_id, public_key,
        random_operation_id, random_previous_operations, update_operation,
    };

    use super::OperationStore;

    #[rstest]
    #[case::create_operation(create_operation(constants::test_fields(), constants::schema().id().to_owned()))]
    #[case::update_operation(update_operation(constants::test_fields(), constants::HASH.parse().unwrap(), constants::schema().id().to_owned()))]
    #[case::update_operation_many_prev_ops(update_operation(constants::test_fields(), random_previous_operations(12), constants::schema().id().to_owned()))]
    #[case::delete_operation(delete_operation(constants::HASH.parse().unwrap(), constants::schema().id().to_owned()))]
    #[case::delete_operation_many_prev_ops(delete_operation(random_previous_operations(12), constants::schema().id().to_owned()))]
    #[tokio::test]
    async fn insert_get_operations(
        #[case] operation: Operation,
        #[from(public_key)] public_key: PublicKey,
        operation_id: OperationId,
        document_id: DocumentId,
        #[from(test_db)]
        #[future]
        db: TestDatabase,
    ) {
        let db = db.await;

        // Insert the doggo operation into the db, returns Ok(true) when succesful.
        let result = db
            .store
            .insert_operation(&operation_id, &public_key, &operation, &document_id)
            .await;
        assert!(result.is_ok());

        // Request the previously inserted operation by it's id.
        let returned_operation = db
            .store
            .get_operation_by_id(&operation_id)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(returned_operation.public_key(), &public_key);
        assert_eq!(returned_operation.fields(), operation.fields());
        assert_eq!(returned_operation.id(), &operation_id);
    }

    #[rstest]
    #[tokio::test]
    async fn insert_operation_twice(
        #[from(create_operation)] operation: Operation,
        public_key: PublicKey,
        operation_id: OperationId,
        document_id: DocumentId,
        #[from(test_db)]
        #[future]
        db: TestDatabase,
    ) {
        let db = db.await;

        assert!(db
            .store
            .insert_operation(&operation_id, &public_key, &operation, &document_id)
            .await
            .is_ok());

        assert_eq!(
            db.store.insert_operation(&operation_id, &public_key, &operation, &document_id).await.unwrap_err().to_string(),
            format!("Error occured when inserting an operation with id OperationId(Hash(\"{}\")) into storage", operation_id.as_str())
        )
    }

    #[rstest]
    #[tokio::test]
    async fn gets_document_by_operation_id(
        #[from(create_operation)] create_operation: Operation,
        #[from(random_operation_id)] create_operation_id: OperationId,
        #[from(update_operation)] update_operation: Operation,
        #[from(random_operation_id)] update_operation_id: OperationId,
        public_key: PublicKey,
        document_id: DocumentId,
        #[from(test_db)]
        #[future]
        db: TestDatabase,
    ) {
        let db = db.await;

        assert!(db
            .store
            .get_document_by_operation_id(&create_operation_id)
            .await
            .unwrap()
            .is_none());

        db.store
            .insert_operation(
                &create_operation_id,
                &public_key,
                &create_operation,
                &document_id,
            )
            .await
            .unwrap();

        assert_eq!(
            db.store
                .get_document_by_operation_id(&create_operation_id)
                .await
                .unwrap()
                .unwrap(),
            document_id.clone()
        );

        db.store
            .insert_operation(
                &update_operation_id,
                &public_key,
                &update_operation,
                &document_id,
            )
            .await
            .unwrap();

        assert_eq!(
            db.store
                .get_document_by_operation_id(&update_operation_id)
                .await
                .unwrap()
                .unwrap(),
            document_id.clone()
        );
    }

    #[rstest]
    #[tokio::test]
    async fn get_operations_by_document_id(
        key_pair: KeyPair,
        #[from(test_db)]
        #[with(5, 1, 1)]
        #[future]
        db: TestDatabase,
    ) {
        let db = db.await;

        let public_key = key_pair.public_key();

        let latest_entry = db
            .store
            .get_latest_entry(&public_key, &LogId::default())
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
