// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;
use std::convert::TryInto;

use async_trait::async_trait;

use crate::document::{Document, DocumentId};
use crate::operation::traits::AsOperation;
use crate::schema::SchemaId;
use crate::storage_provider::error::DocumentStorageError;
use crate::storage_provider::traits::{DocumentStore, OperationStore};
use crate::test_utils::db::{MemoryStore, PublishedOperation};

#[async_trait]
impl DocumentStore for MemoryStore {
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

    async fn get_documents_by_schema(
        &self,
        schema_id: &SchemaId,
    ) -> Result<Vec<Document>, DocumentStorageError> {
        let mut operations_by_document: HashMap<&DocumentId, Vec<PublishedOperation>> =
            HashMap::new();

        let operations = self.operations.lock().unwrap();

        operations
            .iter()
            .filter(|(_, (_, operation))| &operation.schema_id() == schema_id)
            .for_each(|(_, (document_id, operation))| {
                if let Some(operations) = operations_by_document.get_mut(document_id) {
                    operations.push(operation.to_owned())
                } else {
                    operations_by_document.insert(document_id, vec![operation.to_owned()]);
                }
            });

        let documents = operations_by_document
            .values()
            .filter_map(|operations| operations.try_into().ok())
            .collect();

        Ok(documents)
    }
}
#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use rstest::rstest;

    use crate::document::DocumentId;
    use crate::operation::{OperationAction, OperationBuilder, OperationId, OperationValue};
    use crate::schema::SchemaId;
    use crate::storage_provider::traits::{DocumentStore, OperationStore};
    use crate::test_utils::constants::{self, test_fields};
    use crate::test_utils::db::test_db::{test_db, TestDatabase};
    use crate::test_utils::fixtures::{random_document_id, random_operation_id, schema_id};

    #[rstest]
    #[tokio::test]
    async fn gets_one_document(
        #[from(test_db)]
        #[with(1, 1, 1)]
        #[future]
        db: TestDatabase,
    ) {
        let db = db.await;
        let document_id = db.test_data.documents[0].clone();

        let document = db.store.get_document(&document_id).await.unwrap().unwrap();

        for (key, value) in test_fields() {
            assert!(document.get(key).is_some());
            assert_eq!(document.get(key).unwrap(), &value);
        }
    }

    #[rstest]
    #[tokio::test]
    async fn document_does_not_exist(
        random_document_id: DocumentId,
        #[from(test_db)]
        #[with(1, 1, 1)]
        #[future]
        db: TestDatabase,
    ) {
        let db = db.await;
        let document = db.store.get_document(&random_document_id).await.unwrap();

        assert!(document.is_none());
    }

    #[rstest]
    #[tokio::test]
    async fn updates_a_document(
        schema_id: SchemaId,
        #[from(random_operation_id)] operation_id: OperationId,
        #[from(test_db)]
        #[with(1, 1, 1)]
        #[future]
        db: TestDatabase,
    ) {
        let db = db.await;
        let public_key = db.test_data.key_pairs[0].public_key();
        let document_id = db.test_data.documents[0].clone();
        let create_operation_id: OperationId = document_id.as_str().parse().unwrap();

        let field_to_update = ("age", OperationValue::Integer(29));
        let update_operation = OperationBuilder::new(&schema_id)
            .action(OperationAction::Update)
            .previous(&create_operation_id.into())
            .fields(&[field_to_update.clone()])
            .build()
            .unwrap();

        let _ = db
            .store
            .insert_operation(&operation_id, &public_key, &update_operation, &document_id)
            .await
            .is_ok();

        let document = db.store.get_document(&document_id).await.unwrap().unwrap();

        assert!(document.get(field_to_update.0).is_some());
        assert_eq!(document.get(field_to_update.0).unwrap(), &field_to_update.1);
    }

    #[rstest]
    #[tokio::test]
    async fn gets_documents_by_schema(
        #[from(test_db)]
        #[with(10, 2, 1, false, constants::schema())]
        #[future]
        db: TestDatabase,
    ) {
        let db = db.await;
        let schema_id = SchemaId::from_str(constants::SCHEMA_ID).unwrap();

        let schema_documents = db.store.get_documents_by_schema(&schema_id).await.unwrap();

        assert_eq!(schema_documents.len(), 2);

        let schema_documents = db
            .store
            .get_documents_by_schema(&SchemaId::SchemaDefinition(1))
            .await
            .unwrap();

        assert_eq!(schema_documents.len(), 0);
    }
}
