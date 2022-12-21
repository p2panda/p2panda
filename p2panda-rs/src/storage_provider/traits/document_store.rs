// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;

use crate::document::{Document, DocumentId};
use crate::schema::SchemaId;
use crate::storage_provider::error::DocumentStorageError;

/// Storage traits for documents and document views.
#[async_trait]
pub trait DocumentStore {
    async fn get_document(&self, id: &DocumentId)
        -> Result<Option<Document>, DocumentStorageError>;

    /// Get the most recent view for all documents which follow the passed schema.
    ///
    /// Returns a vector of `DocumentView`, or an empty vector if none were found. Returns
    /// an error when a fatal storage error occured.  
    async fn get_documents_by_schema(
        &self,
        schema_id: &SchemaId,
    ) -> Result<Vec<Document>, DocumentStorageError>;
}
