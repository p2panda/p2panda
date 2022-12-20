// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;
use log::debug;

use crate::document::{Document, DocumentId, DocumentView, DocumentViewId};
use crate::entry::traits::{AsEncodedEntry, AsEntry};
use crate::hash::Hash;
use crate::schema::SchemaId;
use crate::storage_provider::error::DocumentStorageError;
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
        debug!(
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
        debug!("Inserting document with id {} into store", document.id());

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

    async fn get_document_by_entry(
        &self,
        entry_hash: &Hash,
    ) -> Result<Option<DocumentId>, DocumentStorageError> {
        let entries = self.entries.lock().unwrap();

        let entry = entries
            .iter()
            .find(|(_, entry)| entry.hash() == *entry_hash);

        let entry = match entry {
            Some((_, entry)) => entry,
            None => return Ok(None),
        };

        let logs = self.logs.lock().unwrap();

        let log = logs.iter().find(|(_, (public_key, log_id, _, _))| {
            log_id == entry.log_id() && public_key == entry.public_key()
        });

        Ok(log.map(|(_, (_, _, _, document_id))| document_id.to_owned()))
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
    use std::convert::TryFrom;
    use std::str::FromStr;

    use rstest::rstest;

    use crate::document::{
        Document, DocumentBuilder, DocumentView, DocumentViewFields, DocumentViewId,
    };
    use crate::entry::traits::AsEncodedEntry;
    use crate::entry::{LogId, SeqNum};
    use crate::operation::traits::AsOperation;
    use crate::operation::OperationId;
    use crate::schema::SchemaId;
    use crate::storage_provider::traits::{DocumentStore, EntryStore, OperationStore};
    use crate::test_utils::constants::{self, test_fields};
    use crate::test_utils::db::test_db::{test_db, TestDatabase};
    use crate::test_utils::db::PublishedOperation;
    use crate::test_utils::fixtures::random_document_view_id;

    #[rstest]
    #[tokio::test]
    async fn inserts_gets_one_document_view(
        #[from(test_db)]
        #[with(1, 1, 1)]
        #[future]
        db: TestDatabase,
    ) {
        let db = db.await;
        let public_key = db.test_data.key_pairs[0].public_key();

        // Get one entry from the pre-polulated db
        let entry = db
            .store
            .get_entry_at_seq_num(&public_key, &LogId::default(), &SeqNum::new(1).unwrap())
            .await
            .unwrap()
            .unwrap();

        let operation = db
            .store
            .get_operation_by_id(&entry.hash().into())
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
                &operation.fields().unwrap(),
            ),
        );

        // Insert into db
        let result = db
            .store
            .insert_document_view(
                &document_view,
                &SchemaId::from_str(constants::SCHEMA_ID).unwrap(),
            )
            .await;

        assert!(result.is_ok());

        let retrieved_document_view = db
            .store
            .get_document_view_by_id(&document_view_id)
            .await
            .unwrap()
            .unwrap();

        for (key, _) in test_fields() {
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
        db: TestDatabase,
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
    async fn inserts_gets_documents(
        #[from(test_db)]
        #[with(1, 1, 1)]
        #[future]
        db: TestDatabase,
    ) {
        let db = db.await;
        let document_id = db.test_data.documents[0].clone();

        let operations: Vec<PublishedOperation> = db
            .store
            .get_operations_by_document_id(&document_id)
            .await
            .unwrap();

        let document = Document::try_from(&operations).unwrap();

        let result = db.store.insert_document(&document).await;

        assert!(result.is_ok());

        let document_view = db
            .store
            .get_document_view_by_id(document.view_id())
            .await
            .unwrap()
            .unwrap();

        let expected_document_view = document.view().unwrap();

        for (key, _) in test_fields() {
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
        db: TestDatabase,
    ) {
        let db = db.await;
        let document_id = db.test_data.documents[0].clone();

        let operations = db
            .store
            .get_operations_by_document_id(&document_id)
            .await
            .unwrap();

        let document = Document::try_from(&operations).unwrap();
        let result = db.store.insert_document(&document).await;

        assert!(result.is_ok());

        let document_view = db
            .store
            .get_document_by_id(document.id())
            .await
            .unwrap()
            .unwrap();

        let expected_document_view = document.view().unwrap();

        for (key, _) in test_fields() {
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
        db: TestDatabase,
    ) {
        let db = db.await;
        let document_id = db.test_data.documents[0].clone();

        let operations = db
            .store
            .get_operations_by_document_id(&document_id)
            .await
            .unwrap();

        let document = Document::try_from(&operations).unwrap();

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
        db: TestDatabase,
    ) {
        let db = db.await;
        let document_id = db.test_data.documents[0].clone();

        let operations = db
            .store
            .get_operations_by_document_id(&document_id)
            .await
            .unwrap();

        let document = Document::try_from(&operations).unwrap();
        let result = db.store.insert_document(&document).await;

        assert!(result.is_ok());

        let document_views = db
            .store
            .get_documents_by_schema(&constants::SCHEMA_ID.parse().unwrap())
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
        db: TestDatabase,
    ) {
        let db = db.await;
        let document_id = db.test_data.documents[0].clone();

        let operations = db
            .store
            .get_operations_by_document_id(&document_id)
            .await
            .unwrap();

        let document = Document::try_from(&operations).unwrap();

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
        #[with(10, 2, 1, false, constants::schema())]
        #[future]
        db: TestDatabase,
    ) {
        let db = db.await;
        let schema_id = SchemaId::from_str(constants::SCHEMA_ID).unwrap();

        for document_id in &db.test_data.documents {
            let operations = db
                .store
                .get_operations_by_document_id(document_id)
                .await
                .unwrap();

            let document = Document::try_from(&operations).unwrap();

            db.store.insert_document(&document).await.unwrap();
        }

        let schema_documents = db.store.get_documents_by_schema(&schema_id).await.unwrap();

        assert_eq!(schema_documents.len(), 2);
    }
}
