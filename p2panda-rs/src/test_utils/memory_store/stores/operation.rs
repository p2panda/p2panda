// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;
use log::debug;

use crate::document::DocumentId;
use crate::operation_v2::body::traits::Schematic;
use crate::operation_v2::header::traits::Actionable;
use crate::operation_v2::traits::AsOperation;
use crate::operation_v2::{Operation, OperationId};
use crate::schema::SchemaId;
use crate::storage_provider::error::OperationStorageError;
use crate::storage_provider::traits::OperationStore;
use crate::test_utils::memory_store::MemoryStore;

#[async_trait]
impl OperationStore for MemoryStore {
    type Operation = Operation;

    /// Insert an `Operation` into the store.
    async fn insert_operation(&self, operation: &Operation) -> Result<(), OperationStorageError> {
        let id = operation.id();
        debug!(
            "Inserting {} operation: {} into store",
            operation.action().as_str(),
            id,
        );

        let mut operations = self.operations.lock().unwrap();

        let is_duplicate_id = operations.values().any(|operation| operation.id() == id);

        if is_duplicate_id {
            return Err(OperationStorageError::InsertionError(id.clone()));
        }

        operations.insert(id.clone(), operation.to_owned());

        Ok(())
    }

    /// Get an `Operation` identified by it's `OperationId`, returns `None` if no `Operation` was found.
    async fn get_operation(
        &self,
        id: &OperationId,
    ) -> Result<Option<Operation>, OperationStorageError> {
        let operations = self.operations.lock().unwrap();
        let operation = operations.get(id).cloned();
        Ok(operation)
    }

    /// Get the `DocumentId` for an `Operation`.
    async fn get_document_id_by_operation_id(
        &self,
        id: &OperationId,
    ) -> Result<Option<DocumentId>, OperationStorageError> {
        let operations = self.operations.lock().unwrap();
        let document_id = operations.values().find_map(|operation| {
            if operation.id() == id {
                Some(operation.document_id())
            } else {
                None
            }
        });
        Ok(document_id.cloned())
    }

    /// Get all `Operations` for a single `Document`.
    async fn get_operations_by_document_id(
        &self,
        id: &DocumentId,
    ) -> Result<Vec<Operation>, OperationStorageError> {
        let operations = self.operations.lock().unwrap();
        let operations = operations
            .values()
            .filter(|operation| operation.document_id() == id)
            .cloned()
            .collect();
        Ok(operations)
    }

    /// Get all `Operations` for a certain `Schema`.
    async fn get_operations_by_schema_id(
        &self,
        id: &SchemaId,
    ) -> Result<Vec<Operation>, OperationStorageError> {
        let operations = self.operations.lock().unwrap();
        Ok(operations
            .values()
            .filter_map(|operation| {
                if operation.schema_id() == id {
                    Some(operation.to_owned())
                } else {
                    None
                }
            })
            .collect())
    }
}
//
// #[cfg(test)]
// mod tests {
//     use rstest::rstest;
//
//     use crate::document::DocumentId;
//     use crate::entry::traits::AsEncodedEntry;
//     use crate::entry::LogId;
//     use crate::identity::PublicKey;
//     use crate::operation::traits::{AsOperation, WithPublicKey};
//     use crate::operation::{Operation, OperationId};
//     use crate::test_utils::constants;
//     use crate::test_utils::fixtures::{
//         create_operation, delete_operation, document_id, operation_id, populate_store_config,
//         public_key, random_operation_id, random_previous_operations, update_operation,
//     };
//     use crate::test_utils::memory_store::helpers::{populate_store, PopulateStoreConfig};
//     use crate::test_utils::memory_store::MemoryStore;
//     use crate::WithId;
//
//     use super::OperationStore;
//
//     #[rstest]
//     #[case::create_operation(create_operation(constants::test_fields(), constants::schema().id().to_owned()))]
//     #[case::update_operation(update_operation(constants::test_fields(), constants::HASH.parse().unwrap(), constants::schema().id().to_owned()))]
//     #[case::update_operation_many_prev_ops(update_operation(constants::test_fields(), random_previous_operations(12), constants::schema().id().to_owned()))]
//     #[case::delete_operation(delete_operation(constants::HASH.parse().unwrap(), constants::schema().id().to_owned()))]
//     #[case::delete_operation_many_prev_ops(delete_operation(random_previous_operations(12), constants::schema().id().to_owned()))]
//     #[tokio::test]
//     async fn insert_get_operations(
//         #[case] operation: Operation,
//         #[from(public_key)] public_key: PublicKey,
//         operation_id: OperationId,
//         document_id: DocumentId,
//     ) {
//         let store = MemoryStore::default();
//
//         // Insert the doggo operation into the db, returns Ok(true) when succesful.
//         let result = store
//             .insert_operation(&operation_id, &public_key, &operation, &document_id)
//             .await;
//         assert!(result.is_ok());
//
//         // Request the previously inserted operation by it's id.
//         let returned_operation = store.get_operation(&operation_id).await.unwrap().unwrap();
//
//         assert_eq!(returned_operation.public_key(), &public_key);
//         assert_eq!(returned_operation.fields(), operation.fields());
//         assert_eq!(
//             WithId::<OperationId>::id(&returned_operation),
//             &operation_id
//         );
//     }
//
//     #[rstest]
//     #[tokio::test]
//     async fn insert_operation_twice(
//         #[from(create_operation)] operation: Operation,
//         public_key: PublicKey,
//         operation_id: OperationId,
//         document_id: DocumentId,
//     ) {
//         let store = MemoryStore::default();
//
//         assert!(store
//             .insert_operation(&operation_id, &public_key, &operation, &document_id)
//             .await
//             .is_ok());
//
//         assert_eq!(
//             store.insert_operation(&operation_id, &public_key, &operation, &document_id).await.unwrap_err().to_string(),
//             format!("Error occured when inserting an operation with id OperationId(Hash(\"{}\")) into storage", operation_id.as_str())
//         )
//     }
//
//     #[rstest]
//     #[tokio::test]
//     async fn gets_document_by_operation_id(
//         #[from(create_operation)] create_operation: Operation,
//         #[from(random_operation_id)] create_operation_id: OperationId,
//         #[from(update_operation)] update_operation: Operation,
//         #[from(random_operation_id)] update_operation_id: OperationId,
//         public_key: PublicKey,
//         document_id: DocumentId,
//     ) {
//         let store = MemoryStore::default();
//
//         assert!(store
//             .get_document_id_by_operation_id(&create_operation_id)
//             .await
//             .unwrap()
//             .is_none());
//
//         store
//             .insert_operation(
//                 &create_operation_id,
//                 &public_key,
//                 &create_operation,
//                 &document_id,
//             )
//             .await
//             .unwrap();
//
//         assert_eq!(
//             store
//                 .get_document_id_by_operation_id(&create_operation_id)
//                 .await
//                 .unwrap()
//                 .unwrap(),
//             document_id.clone()
//         );
//
//         store
//             .insert_operation(
//                 &update_operation_id,
//                 &public_key,
//                 &update_operation,
//                 &document_id,
//             )
//             .await
//             .unwrap();
//
//         assert_eq!(
//             store
//                 .get_document_id_by_operation_id(&update_operation_id)
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
//         #[from(populate_store_config)]
//         #[with(5, 1, 1)]
//         config: PopulateStoreConfig,
//     ) {
//         let store = MemoryStore::default();
//         let (key_pairs, _) = populate_store(&store, &config).await;
//
//         let public_key = key_pairs[0].public_key();
//
//         let latest_entry = store
//             .get_latest_entry(&public_key, &LogId::default())
//             .await
//             .unwrap()
//             .unwrap();
//
//         let document_id = store
//             .get_document_id_by_operation_id(&latest_entry.hash().into())
//             .await
//             .unwrap()
//             .unwrap();
//
//         let operations_by_document_id = store
//             .get_operations_by_document_id(&document_id)
//             .await
//             .unwrap();
//
//         assert_eq!(operations_by_document_id.len(), 5)
//     }
// }
