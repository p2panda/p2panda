// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;
use std::convert::TryInto;

use async_trait::async_trait;

use crate::document::{Document, DocumentBuilder, DocumentId, DocumentViewId};
use crate::operation::traits::AsOperation;
use crate::schema::SchemaId;
use crate::storage_provider::error::DocumentStorageError;
use crate::storage_provider::traits::{DocumentStore, OperationStore};
use crate::test_utils::memory_store::MemoryStore;
use crate::WithId;

#[async_trait]
impl DocumentStore for MemoryStore {
    /// Associated type representing an `Entry` retrieved from storage.
    type Document = Document;

    async fn get_document(
        &self,
        id: &DocumentId,
    ) -> Result<Option<Document>, DocumentStorageError> {
        let operations = self.get_operations_by_document_id(id).await?;

        if operations.is_empty() {
            return Ok(None);
        }

        Ok(Some({ &operations }.try_into()?))
    }

    async fn get_document_by_view_id(
        &self,
        id: &DocumentViewId,
    ) -> Result<Option<Document>, DocumentStorageError> {
        let operation_id = id.iter().next().unwrap();
        let document_id = match self.get_document_id_by_operation_id(operation_id).await? {
            Some(id) => id,
            None => return Ok(None),
        };

        let operations = self.get_operations_by_document_id(&document_id).await?;

        if operations.is_empty() {
            return Ok(None);
        }

        let document_builder: DocumentBuilder = (&operations).into();

        Ok(Some(document_builder.build_to_view_id(Some(id.clone()))?))
    }

    async fn get_documents_by_schema(
        &self,
        schema_id: &SchemaId,
    ) -> Result<Vec<Document>, DocumentStorageError> {
        let mut operations_by_document: HashMap<_, Vec<_>> = HashMap::new();

        let operations = self.get_operations_by_schema_id(schema_id).await?;

        operations
            .iter()
            .filter(|operation| &operation.schema_id() == schema_id)
            .for_each(|operation| {
                let document_id = WithId::<DocumentId>::id(operation);
                match operations_by_document.get_mut(document_id) {
                    Some(operations) => operations.push(operation),
                    None => {
                        operations_by_document.insert(document_id, vec![operation]);
                    }
                }
            });

        let documents = operations_by_document
            .values()
            .filter_map(|operations| operations.clone().try_into().ok())
            .collect();

        Ok(documents)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use rstest::rstest;

    use crate::document::traits::AsDocument;
    use crate::document::DocumentId;
    use crate::operation::{OperationAction, OperationBuilder, OperationId, OperationValue};
    use crate::schema::SchemaId;
    use crate::storage_provider::traits::{DocumentStore, OperationStore};
    use crate::test_utils::constants::{self, test_fields};
    use crate::test_utils::fixtures::{
        populate_store_config, random_document_id, random_operation_id, schema_id,
    };
    use crate::test_utils::memory_store::helpers::{populate_store, PopulateStoreConfig};
    use crate::test_utils::memory_store::MemoryStore;

    #[rstest]
    #[tokio::test]
    async fn gets_one_document(
        #[from(populate_store_config)]
        #[with(1, 1, 1)]
        config: PopulateStoreConfig,
    ) {
        let store = MemoryStore::default();
        let (_, documents) = populate_store(&store, &config).await;
        let document_id = documents[0].clone();

        let document = store.get_document(&document_id).await.unwrap().unwrap();

        for (key, value) in test_fields() {
            assert!(document.get(key).is_some());
            assert_eq!(document.get(key).unwrap(), &value);
        }
    }

    #[rstest]
    #[tokio::test]
    async fn document_does_not_exist(
        random_document_id: DocumentId,
        #[from(populate_store_config)]
        #[with(1, 1, 1)]
        config: PopulateStoreConfig,
    ) {
        let store = MemoryStore::default();
        populate_store(&store, &config).await;
        let document = store.get_document(&random_document_id).await.unwrap();

        assert!(document.is_none());
    }

    #[rstest]
    #[tokio::test]
    async fn updates_a_document(
        schema_id: SchemaId,
        #[from(random_operation_id)] operation_id: OperationId,
        #[from(populate_store_config)]
        #[with(1, 1, 1)]
        config: PopulateStoreConfig,
    ) {
        let store = MemoryStore::default();
        let (key_pairs, documents) = populate_store(&store, &config).await;

        let public_key = key_pairs[0].public_key();
        let document_id = documents[0].clone();
        let create_operation_id: OperationId = document_id.as_str().parse().unwrap();

        let field_to_update = ("age", OperationValue::Integer(29));
        let update_operation = OperationBuilder::new(&schema_id)
            .action(OperationAction::Update)
            .previous(&create_operation_id.into())
            .fields(&[field_to_update.clone()])
            .build()
            .unwrap();

        let _ = store
            .insert_operation(&operation_id, &public_key, &update_operation, &document_id)
            .await
            .is_ok();

        let document = store.get_document(&document_id).await.unwrap().unwrap();

        assert!(document.get(field_to_update.0).is_some());
        assert_eq!(document.get(field_to_update.0).unwrap(), &field_to_update.1);
    }

    #[rstest]
    #[tokio::test]
    async fn gets_documents_by_schema(
        #[from(populate_store_config)]
        #[with(10, 2, 1, false, constants::schema())]
        config: PopulateStoreConfig,
    ) {
        let store = MemoryStore::default();
        populate_store(&store, &config).await;

        let schema_id = SchemaId::from_str(constants::SCHEMA_ID).unwrap();
        let schema_documents = store.get_documents_by_schema(&schema_id).await.unwrap();

        assert_eq!(schema_documents.len(), 2);

        let schema_documents = store
            .get_documents_by_schema(&SchemaId::SchemaDefinition(1))
            .await
            .unwrap();

        assert_eq!(schema_documents.len(), 0);
    }
}
