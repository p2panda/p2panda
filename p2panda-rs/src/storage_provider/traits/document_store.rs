// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;

use crate::document::traits::AsDocument;
use crate::document::{DocumentId, DocumentViewId};
use crate::schema::SchemaId;
use crate::storage_provider::error::DocumentStorageError;
use crate::storage_provider::traits::OperationStore;

/// Interface for querying `Documents` from the store.
///
/// `Documents` are the high level data types most applications will concern themselves with. They
/// are mutated by peers publishing operations which contain changes to one or many of the documents
/// key-value pairs.
///
/// We call the process of turning a bunch of operations into a document materialisation. The interface
/// outlined in these traits offers a simple API for querying documents by their id or schema. As each
/// node implementation may approach materialising documents in different ways, it is expected that the
/// storage API for documents will be expanded to suit their needs.
#[async_trait]
pub trait DocumentStore: OperationStore {
    /// Associated type representing an `Entry` retrieved from storage.
    type Document: AsDocument;

    /// Get a document by it's `DocumentId`.
    ///
    /// Returns a result containing a document if one is found. Errors when a fatal storage
    /// error occurs.
    async fn get_document(
        &self,
        id: &DocumentId,
    ) -> Result<Option<Self::Document>, DocumentStorageError>;

    /// Get a document by it's `DocumentViewId`.
    /// 
    /// This returns the document materialised to the state identified by the passed `DocumentViewId`.
    ///
    /// Returns a result containing a document if one is found. Errors when a fatal storage
    /// error occurs.
    async fn get_document_by_view_id(
        &self,
        id: &DocumentViewId,
    ) -> Result<Option<Self::Document>, DocumentStorageError>;

    /// Get all documents which contain data published under the passed schema.
    ///
    /// Returns a result containing a collection of documents, can be emply if no documents were
    /// published under the passed schema or if the schema itself was not found. Errors when a fatal storage
    /// error occurs.
    async fn get_documents_by_schema(
        &self,
        schema_id: &SchemaId,
    ) -> Result<Vec<Self::Document>, DocumentStorageError>;
}
