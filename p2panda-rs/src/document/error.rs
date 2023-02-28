// SPDX-License-Identifier: AGPL-3.0-or-later

//! Error types for creating or materializing documents and document views and validating the
//! format of document ids and document view ids.
use thiserror::Error;

use crate::operation::OperationId;

/// Error types for methods of `DocumentBuilder` struct.
#[derive(Error, Debug)]
pub enum DocumentBuilderError {
    /// No create operation found.
    #[error("every document must contain one create operation")]
    NoCreateOperation,

    /// A document can only have one create operation.
    #[error("multiple CREATE operations found")]
    MoreThanOneCreateOperation,

    /// All operations in a document must follow the same schema.
    #[error("all operations in a document must follow the same schema")]
    OperationSchemaNotMatching,

    /// To resolve a document the schema must be set.
    #[error("Schema must be set")]
    SchemaMustBeSet,

    /// An operation with invalid id or previous operations was added to the document.
    #[error("operation {0} cannot be connected to the document graph")]
    InvalidOperationLink(OperationId),

    /// Handle errors from validating CBOR schemas.
    #[error(transparent)]
    DocumentViewError(#[from] DocumentViewError),

    /// Handle errors when sorting the graph.
    #[error(transparent)]
    GraphSortingError(#[from] crate::graph::error::GraphError),
}

/// Error types for methods of `Document` struct.
#[derive(Error, Debug)]
pub enum DocumentError {
    /// Operation passed to commit does not refer to this documents current view.
    #[error("operation {0} does not update the documents current view")]
    PreviousDoesNotMatch(OperationId),

    /// Operation passed to commit has incorrect operation type.
    #[error("CREATE operation used to update document")]
    InvalidOperationType,

    /// Operation passed to commit does not the same schema as the document.
    #[error("Operation {0} does not match the documents schema")]
    InvalidSchemaId(OperationId),

    /// Cannot perform a commit on a deleted document.
    #[error("Cannot perform a commit on a deleted document")]
    UpdateOnDeleted,

    /// Handle errors from validating CBOR schemas.
    #[error(transparent)]
    DocumentViewError(#[from] DocumentViewError),

    /// Handle errors when sorting the graph.
    #[error(transparent)]
    GraphSortingError(#[from] crate::graph::error::GraphError),
}

/// Custom error types for `DocumentView`.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum DocumentViewError {
    /// TryFrom operation must be CREATE.
    #[error("operation must be instantiated from a CREATE operation")]
    NotCreateOperation,

    /// Operation passed to `update()` must be UPDATE or DELETE.
    #[error("operation passed to update() must be UPDATE or DELETE")]
    NotUpdateOrDeleteOperation,
}

/// Error types for `DocumentViewId`.
#[derive(Error, Debug)]
pub enum DocumentViewIdError {
    /// Document view ids must contain sorted operation ids.
    #[error("expected sorted operation ids in document view id")]
    UnsortedOperationIds,

    /// Document view ids must contain at least one operation ids.
    #[error("expected one or more operation ids")]
    ZeroOperationIds,

    /// Handle errors from validating operation id hashes.
    #[error(transparent)]
    InvalidOperationId(#[from] crate::operation::error::OperationIdError),
}

/// Error types for `DocumentId`.
#[derive(Error, Debug)]
pub enum DocumentIdError {
    /// Handle errors from validating operation ids.
    #[error(transparent)]
    OperationIdError(#[from] crate::operation::error::OperationIdError),
}
