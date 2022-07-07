// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;
use log::info;

use crate::document::{Document, DocumentId, DocumentView, DocumentViewId};
use crate::schema::SchemaId;
use crate::storage_provider::errors::DocumentStorageError;
use crate::storage_provider::traits::DocumentStore;
use crate::test_utils::db::MemoryStore;

#[async_trait]
impl DocumentStore for MemoryStore {
    /// Insert document view into storage.
    ///
    /// returns an error when a fatal storage error occurs.
    async fn insert_document_view(
        &self,
        document_view: &DocumentView,
        schema_id: &SchemaId,
    ) -> Result<(), DocumentStorageError> {
        info!(
            "Inserting document view with id {} into store",
            document_view.id()
        );
        self.document_views.lock().unwrap().insert(
            document_view.id().to_owned(),
            (schema_id.to_owned(), document_view.to_owned()),
        );

        Ok(())
    }

    /// Get a document view from storage by it's `DocumentViewId`.
    ///
    /// Returns a DocumentView or `None` if no view was found with this id. Returns
    /// an error if a fatal storage error occured.
    async fn get_document_view_by_id(
        &self,
        id: &DocumentViewId,
    ) -> Result<Option<DocumentView>, DocumentStorageError> {
        let view = self
            .document_views
            .lock()
            .unwrap()
            .get(id)
            .map(|(_, document_view)| document_view.to_owned());
        Ok(view)
    }

    /// Insert a document into storage.
    ///
    /// Inserts a document into storage and should retain a pointer to it's most recent
    /// document view. Returns an error if a fatal storage error occured.
    async fn insert_document(&self, document: &Document) -> Result<(), DocumentStorageError> {
        info!("Inserting document with id {} into store", document.id());

        self.documents
            .lock()
            .unwrap()
            .insert(document.id().to_owned(), document.to_owned());

        if !document.is_deleted() {
            self.insert_document_view(document.view().unwrap(), document.schema())
                .await?;
        }

        Ok(())
    }

    /// Get the lates document view for a document identified by it's `DocumentId`.
    ///
    /// Returns a type implementing `AsDocumentView` wrapped in an `Option`, returns
    /// `None` if no view was found with this document. Returns an error if a fatal storage error
    /// occured.
    ///
    /// Note: if no view for this document was found, it might have been deleted.
    async fn get_document_by_id(
        &self,
        id: &DocumentId,
    ) -> Result<Option<DocumentView>, DocumentStorageError> {
        match self
            .documents
            .lock()
            .unwrap()
            .get(id)
            .map(|document| document.to_owned())
        {
            Some(document) => Ok(document.view().map(|view| view.to_owned())),
            None => Ok(None),
        }
    }

    /// Get the most recent view for all documents which follow the passed schema.
    ///
    /// Returns a vector of `DocumentView`, or an empty vector if none were found. Returns
    /// an error when a fatal storage error occured.  
    async fn get_documents_by_schema(
        &self,
        schema_id: &SchemaId,
    ) -> Result<Vec<DocumentView>, DocumentStorageError> {
        let documents: Vec<DocumentView> = self
            .documents
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, document)| document.schema() == schema_id)
            .filter_map(|(_, document)| document.view().cloned())
            .collect();

        Ok(documents)
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use std::convert::TryFrom;
    use std::str::FromStr;

    use crate::document::{
        DocumentBuilder, DocumentView, DocumentViewFields, DocumentViewId, DocumentViewValue,
    };
    use crate::entry::{LogId, SeqNum};
    use crate::identity::Author;
    use crate::operation::{AsOperation, OperationId, OperationValue};
    use crate::schema::SchemaId;
    use crate::storage_provider::traits::test_utils::{test_db, TestStore};
    use crate::storage_provider::traits::{
        AsStorageEntry, DocumentStore, EntryStore, OperationStore,
    };
    use crate::test_utils::constants::SCHEMA_ID;
    use crate::test_utils::db::StorageEntry;
    use crate::test_utils::fixtures::random_document_view_id;

    fn entries_to_document_views(entries: &[StorageEntry]) -> Vec<DocumentView> {
        let mut document_views = Vec::new();
        let mut current_document_view_fields = DocumentViewFields::new();

        for entry in entries {
            let operation_id: OperationId = entry.hash().into();

            for (name, value) in entry.operation().fields().unwrap().iter() {
                if entry.operation().is_delete() {
                    continue;
                } else {
                    current_document_view_fields
                        .insert(name, DocumentViewValue::new(&operation_id, value));
                }
            }

            let document_view_fields = DocumentViewFields::new_from_operation_fields(
                &operation_id,
                &entry.operation().fields().unwrap(),
            );

            let document_view =
                DocumentView::new(&operation_id.clone().into(), &document_view_fields);

            document_views.push(document_view)
        }

        document_views
    }

    #[rstest]
    #[tokio::test]
    async fn inserts_gets_one_document_view(
        #[from(test_db)]
        #[with(1, 1, 1)]
        #[future]
        db: TestStore,
    ) {
        let db = db.await;
        let author = Author::try_from(db.test_data.key_pairs[0].public_key().to_owned()).unwrap();

        // Get one entry from the pre-polulated db
        let entry = db
            .store
            .get_entry_at_seq_num(&author, &LogId::new(1), &SeqNum::new(1).unwrap())
            .await
            .unwrap()
            .unwrap();

        // Construct a `DocumentView`
        let operation_id: OperationId = entry.hash().into();
        let document_view_id: DocumentViewId = operation_id.clone().into();
        let document_view = DocumentView::new(
            &document_view_id,
            &DocumentViewFields::new_from_operation_fields(
                &operation_id,
                &entry.operation().fields().unwrap(),
            ),
        );

        // Insert into db
        let result = db
            .store
            .insert_document_view(&document_view, &SchemaId::from_str(SCHEMA_ID).unwrap())
            .await;

        assert!(result.is_ok());

        let retrieved_document_view = db
            .store
            .get_document_view_by_id(&document_view_id)
            .await
            .unwrap()
            .unwrap();

        for key in [
            "username",
            "age",
            "height",
            "is_admin",
            "profile_picture",
            "many_profile_pictures",
            "special_profile_picture",
            "many_special_profile_pictures",
            "another_relation_field",
        ] {
            assert!(retrieved_document_view.get(key).is_some());
            assert_eq!(retrieved_document_view.get(key), document_view.get(key));
        }
    }

    #[rstest]
    #[tokio::test]
    async fn document_view_does_not_exist(
        random_document_view_id: DocumentViewId,
        #[from(test_db)]
        #[with(1, 1, 1)]
        #[future]
        db: TestStore,
    ) {
        let db = db.await;
        let view_does_not_exist = db
            .store
            .get_document_view_by_id(&random_document_view_id)
            .await
            .unwrap();

        assert!(view_does_not_exist.is_none());
    }

    #[rstest]
    #[tokio::test]
    async fn inserts_gets_many_document_views(
        #[from(test_db)]
        #[with(10, 1, 1, false, SCHEMA_ID.parse().unwrap(), vec![("username", OperationValue::Text("panda".into()))], vec![("username", OperationValue::Text("PANDA".into()))])]
        #[future]
        db: TestStore,
    ) {
        let db = db.await;
        let author = Author::try_from(db.test_data.key_pairs[0].public_key().to_owned()).unwrap();
        let schema_id = SchemaId::from_str(SCHEMA_ID).unwrap();

        let log_id = LogId::default();
        let seq_num = SeqNum::default();

        // Get 10 entries from the pre-populated test db
        let entries = db
            .store
            .get_paginated_log_entries(&author, &log_id, &seq_num, 10)
            .await
            .unwrap();

        // Parse them into document views
        let document_views = entries_to_document_views(&entries);

        // Insert each of these views into the db
        for document_view in document_views.clone() {
            db.store
                .insert_document_view(&document_view, &schema_id)
                .await
                .unwrap();
        }

        // Retrieve them again and assert they are the same as the inserted ones
        for (count, entry) in entries.iter().enumerate() {
            let result = db.store.get_document_view_by_id(&entry.hash().into()).await;

            assert!(result.is_ok());

            let document_view = result.unwrap().unwrap();

            // The update operation should be included in the view correctly, we check that here.
            let expected_username = if count == 0 {
                DocumentViewValue::new(
                    &entry.hash().into(),
                    &OperationValue::Text("panda".to_string()),
                )
            } else {
                DocumentViewValue::new(
                    &entry.hash().into(),
                    &OperationValue::Text("PANDA".to_string()),
                )
            };
            assert_eq!(document_view.get("username").unwrap(), &expected_username);
        }
    }

    #[rstest]
    #[tokio::test]
    async fn inserts_gets_documents(
        #[from(test_db)]
        #[with(1, 1, 1)]
        #[future]
        db: TestStore,
    ) {
        let db = db.await;
        let document_id = db.test_data.documents[0].clone();

        let document_operations = db
            .store
            .get_operations_by_document_id(&document_id)
            .await
            .unwrap();

        let document = DocumentBuilder::new(document_operations).build().unwrap();

        let result = db.store.insert_document(&document).await;

        assert!(result.is_ok());

        let document_view = db
            .store
            .get_document_view_by_id(document.view_id())
            .await
            .unwrap()
            .unwrap();

        let expected_document_view = document.view().unwrap();

        for key in [
            "username",
            "age",
            "height",
            "is_admin",
            "profile_picture",
            "many_profile_pictures",
            "special_profile_picture",
            "many_special_profile_pictures",
            "another_relation_field",
        ] {
            assert!(document_view.get(key).is_some());
            assert_eq!(document_view.get(key), expected_document_view.get(key));
        }
    }

    #[rstest]
    #[tokio::test]
    async fn gets_document_by_id(
        #[from(test_db)]
        #[with(1, 1, 1)]
        #[future]
        db: TestStore,
    ) {
        let db = db.await;
        let document_id = db.test_data.documents[0].clone();

        let document_operations = db
            .store
            .get_operations_by_document_id(&document_id)
            .await
            .unwrap();

        let document = DocumentBuilder::new(document_operations).build().unwrap();

        let result = db.store.insert_document(&document).await;

        assert!(result.is_ok());

        let document_view = db
            .store
            .get_document_by_id(document.id())
            .await
            .unwrap()
            .unwrap();

        let expected_document_view = document.view().unwrap();

        for key in [
            "username",
            "age",
            "height",
            "is_admin",
            "profile_picture",
            "many_profile_pictures",
            "special_profile_picture",
            "many_special_profile_pictures",
            "another_relation_field",
        ] {
            assert!(document_view.get(key).is_some());
            assert_eq!(document_view.get(key), expected_document_view.get(key));
        }
    }

    #[rstest]
    #[tokio::test]
    async fn no_view_when_document_deleted(
        #[from(test_db)]
        #[with(10, 1, 1, true)]
        #[future]
        db: TestStore,
    ) {
        let db = db.await;
        let document_id = db.test_data.documents[0].clone();

        let document_operations = db
            .store
            .get_operations_by_document_id(&document_id)
            .await
            .unwrap();

        let document = DocumentBuilder::new(document_operations).build().unwrap();

        let result = db.store.insert_document(&document).await;

        assert!(result.is_ok());

        let document_view = db.store.get_document_by_id(document.id()).await.unwrap();

        assert!(document_view.is_none());
    }

    #[rstest]
    #[tokio::test]
    async fn get_documents_by_schema_deleted_document(
        #[from(test_db)]
        #[with(10, 1, 1, true)]
        #[future]
        db: TestStore,
    ) {
        let db = db.await;
        let document_id = db.test_data.documents[0].clone();

        let document_operations = db
            .store
            .get_operations_by_document_id(&document_id)
            .await
            .unwrap();

        let document = DocumentBuilder::new(document_operations).build().unwrap();

        let result = db.store.insert_document(&document).await;

        assert!(result.is_ok());

        let document_views = db
            .store
            .get_documents_by_schema(&SCHEMA_ID.parse().unwrap())
            .await
            .unwrap();

        assert!(document_views.is_empty());
    }

    #[rstest]
    #[tokio::test]
    async fn updates_a_document(
        #[from(test_db)]
        #[with(10, 1, 1)]
        #[future]
        db: TestStore,
    ) {
        let db = db.await;
        let document_id = db.test_data.documents[0].clone();

        let document_operations = db
            .store
            .get_operations_by_document_id(&document_id)
            .await
            .unwrap();

        let document = DocumentBuilder::new(document_operations).build().unwrap();

        let mut current_operations = Vec::new();

        for operation in document.operations() {
            // For each operation in the db we insert a document, cumulatively adding the next operation
            // each time. this should perform an "INSERT" first in the documents table, followed by 9 "UPDATES".
            current_operations.push(operation.clone());
            let document = DocumentBuilder::new(current_operations.clone())
                .build()
                .unwrap();
            let result = db.store.insert_document(&document).await;
            assert!(result.is_ok());

            let document_view = db.store.get_document_by_id(document.id()).await.unwrap();
            assert!(document_view.is_some());
        }
    }

    #[rstest]
    #[tokio::test]
    async fn gets_documents_by_schema(
        #[from(test_db)]
        #[with(10, 2, 1, false, SCHEMA_ID.parse().unwrap())]
        #[future]
        db: TestStore,
    ) {
        let db = db.await;
        let schema_id = SchemaId::from_str(SCHEMA_ID).unwrap();

        for document_id in &db.test_data.documents {
            let document_operations = db
                .store
                .get_operations_by_document_id(document_id)
                .await
                .unwrap();

            let document = DocumentBuilder::new(document_operations).build().unwrap();

            db.store.insert_document(&document).await.unwrap();
        }

        let schema_documents = db.store.get_documents_by_schema(&schema_id).await.unwrap();

        assert_eq!(schema_documents.len(), 2);
    }
}
