// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;
use log::debug;

use crate::document::DocumentId;
use crate::operation::traits::{AsOperation, AsVerifiedOperation};
use crate::operation::{OperationId, VerifiedOperation};
use crate::storage_provider::error::OperationStorageError;
use crate::storage_provider::traits::OperationStore;
use crate::test_utils::memory_store::MemoryStore;

#[async_trait]
impl OperationStore<VerifiedOperation> for MemoryStore {
    async fn insert_operation(
        &self,
        operation: &VerifiedOperation,
        document_id: &DocumentId,
    ) -> Result<(), OperationStorageError> {
        debug!(
            "Inserting {} operation: {} into store",
            operation.action().as_str(),
            operation.id(),
        );

        let mut operations = self.operations.lock().unwrap();
        if operations
            .values()
            .any(|(_document_id, verified_operation)| verified_operation == operation)
        {
            return Err(OperationStorageError::InsertionError(
                operation.id().clone(),
            ));
        } else {
            operations.insert(
                operation.id().to_owned(),
                (document_id.clone(), operation.clone()),
            )
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
    ) -> Result<Vec<VerifiedOperation>, OperationStorageError> {
        let operations = self.operations.lock().unwrap();
        Ok(operations
            .values()
            .filter(|(document_id, _verified_operation)| document_id == id)
            .map(|(_, operation)| operation.clone())
            .collect())
    }
}
// TODO: Needs reinstating when we have TestDatabase working again
// #[cfg(test)]
// mod tests {
//     use rstest::rstest;
//
//     use crate::document::DocumentId;
//     use crate::entry::LogId;
//     use crate::identity::{Author, KeyPair};
//     use crate::operation::traits::AsVerifiedOperation;
//     use crate::operation::VerifiedOperation;
//     use crate::storage_provider::traits::test_utils::{test_db, TestStore};
//     use crate::storage_provider::traits::{AsStorageEntry, EntryStore, StorageProvider};
//     use crate::test_utils::fixtures::{document_id, key_pair, verified_operation};
//
//     use super::OperationStore;
//
//     #[rstest]
//     #[case::create_operation(create_operation(&test_fields()))]
//     #[case::update_operation(update_operation(&test_fields(), &HASH.parse().unwrap()))]
//     #[case::update_operation_many_prev_ops(update_operation(&test_fields(), &random_previous_operations(12)))]
//     #[case::delete_operation(delete_operation(&HASH.parse().unwrap()))]
//     #[case::delete_operation_many_prev_ops(delete_operation(&random_previous_operations(12)))]
//     #[tokio::test]
//     async fn insert_get_operations(
//         #[case] operation: Operation,
//         #[from(public_key)] author: Author,
//         operation_id: OperationId,
//         document_id: DocumentId,
//         #[from(test_db)]
//         #[future]
//         db: TestStore,
//     ) {
//         let db = db.await;
//         // Construct the storage operation.
//         let operation = VerifiedOperation::new(&author, &operation_id, &operation).unwrap();
//
//         // Insert the doggo operation into the db, returns Ok(true) when succesful.
//         let result = db.store.insert_operation(&operation, &document_id).await;
//         assert!(result.is_ok());
//
//         // Request the previously inserted operation by it's id.
//         let returned_operation = db
//             .store
//             .get_operation_by_id(operation.id())
//             .await
//             .unwrap()
//             .unwrap();
//
//         assert_eq!(returned_operation.public_key(), operation.public_key());
//         assert_eq!(returned_operation.fields(), operation.fields());
//         assert_eq!(returned_operation.id(), operation.id());
//     }
//
//     #[rstest]
//     #[tokio::test]
//     async fn insert_operation_twice(
//         #[from(verified_operation)] verified_operation: VerifiedOperation,
//         document_id: DocumentId,
//         #[from(test_db)]
//         #[future]
//         db: TestStore,
//     ) {
//         let db = db.await;
//
//         assert!(db
//             .store
//             .insert_operation(&verified_operation, &document_id)
//             .await
//             .is_ok());
//
//         assert_eq!(
//             db.store.insert_operation(&verified_operation, &document_id).await.unwrap_err().to_string(),
//             format!("Error occured when inserting an operation with id OperationId(Hash(\"{}\")) into storage", verified_operation.id().as_str())
//         )
//     }
//
//     #[rstest]
//     #[tokio::test]
//     async fn gets_document_by_operation_id(
//         #[from(verified_operation)]
//         #[with(Some(operation_fields(test_fields())), None, None, None, Some(HASH.parse().unwrap()))]
//         create_operation: VerifiedOperation,
//         #[from(verified_operation)]
//         #[with(Some(operation_fields(test_fields())), Some(HASH.parse().unwrap()))]
//         update_operation: VerifiedOperation,
//         document_id: DocumentId,
//         #[from(test_db)]
//         #[future]
//         db: TestStore,
//     ) {
//         let db = db.await;
//
//         assert!(db
//             .store
//             .get_document_by_operation_id(create_operation.id())
//             .await
//             .unwrap()
//             .is_none());
//
//         db.store
//             .insert_operation(&create_operation, &document_id)
//             .await
//             .unwrap();
//
//         assert_eq!(
//             db.store
//                 .get_document_by_operation_id(create_operation.id())
//                 .await
//                 .unwrap()
//                 .unwrap(),
//             document_id.clone()
//         );
//
//         db.store
//             .insert_operation(&update_operation, &document_id)
//             .await
//             .unwrap();
//
//         assert_eq!(
//             db.store
//                 .get_document_by_operation_id(create_operation.id())
//                 .await
//                 .unwrap()
//                 .unwrap(),
//             document_id.clone()
//         );
//     }
//
//     #[rstest]
//     #[tokio::test]
//     async fn get_operations_by_document_id(
//         key_pair: KeyPair,
//         #[from(test_db)]
//         #[with(5, 1, 1)]
//         #[future]
//         db: TestStore,
//     ) {
//         let db = db.await;
//
//         let author = Author::from(key_pair.public_key());
//
//         let latest_entry = db
//             .store
//             .get_latest_entry(&author, &LogId::default())
//             .await
//             .unwrap()
//             .unwrap();
//
//         let document_id = db
//             .store
//             .get_document_by_entry(&latest_entry.hash())
//             .await
//             .unwrap()
//             .unwrap();
//
//         let operations_by_document_id = db
//             .store
//             .get_operations_by_document_id(&document_id)
//             .await
//             .unwrap();
//
//         assert_eq!(operations_by_document_id.len(), 5)
//     }
// }
