// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;

use crate::document::{Document, DocumentId, DocumentView, DocumentViewId};
use crate::schema::SchemaId;
use crate::storage_provider::error::DocumentStorageError;

/// Storage traits for documents and document views.
#[async_trait]
pub trait DocumentStore {
    /// Insert document view into storage.
    ///
    /// returns an error when a fatal storage error occurs.
    async fn insert_document_view(
        &self,
        document_view: &DocumentView,
        schema_id: &SchemaId,
    ) -> Result<(), DocumentStorageError>;

    /// Get a document view from storage by it's `DocumentViewId`.
    ///
    /// Returns a DocumentView or `None` if no view was found with this id. Returns
    /// an error if a fatal storage error occured.
    async fn get_document_view_by_id(
        &self,
        id: &DocumentViewId,
    ) -> Result<Option<DocumentView>, DocumentStorageError>;

    /// Insert a document into storage.
    ///
    /// Inserts a document into storage and should retain a pointer to it's most recent
    /// document view. Returns an error if a fatal storage error occured.
    async fn insert_document(&self, document: &Document) -> Result<(), DocumentStorageError>;

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
    ) -> Result<Option<DocumentView>, DocumentStorageError>;

    /// Get the most recent view for all documents which follow the passed schema.
    ///
    /// Returns a vector of `DocumentView`, or an empty vector if none were found. Returns
    /// an error when a fatal storage error occured.  
    async fn get_documents_by_schema(
        &self,
        schema_id: &SchemaId,
    ) -> Result<Vec<DocumentView>, DocumentStorageError>;
}
