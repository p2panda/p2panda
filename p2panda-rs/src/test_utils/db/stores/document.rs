// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;

use crate::document::{Document, DocumentId, DocumentView, DocumentViewId};
use crate::schema::SchemaId;
use crate::storage_provider::errors::DocumentStorageError;
use crate::storage_provider::traits::DocumentStore;
use crate::test_utils::db::SimplestStorageProvider;

#[async_trait]
impl DocumentStore for SimplestStorageProvider {
    /// Insert document view into storage.
    ///
    /// returns an error when a fatal storage error occurs.
    async fn insert_document_view(
        &self,
        document_view: &DocumentView,
        schema_id: &SchemaId,
    ) -> Result<(), DocumentStorageError> {
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
            .get(&id)
            .map(|(_, document_view)| document_view.to_owned());
        Ok(view)
    }

    /// Insert a document into storage.
    ///
    /// Inserts a document into storage and should retain a pointer to it's most recent
    /// document view. Returns an error if a fatal storage error occured.
    async fn insert_document(&self, document: &Document) -> Result<(), DocumentStorageError> {
        self.documents
            .lock()
            .unwrap()
            .insert(document.id().to_owned(), document.to_owned());

        if !document.is_deleted() {
            self.document_views.lock().unwrap().insert(
                document.view_id().to_owned(),
                (
                    document.schema().to_owned(),
                    document.view().unwrap().to_owned(),
                ),
            );
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
