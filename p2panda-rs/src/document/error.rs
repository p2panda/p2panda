// SPDX-License-Identifier: AGPL-3.0-or-later

//! Error types for creating or materializing documents and document views and validating the
//! format of document ids and document view ids.
use thiserror::Error;

use crate::operation::OperationId;

/// Error types for methods of `DocumentBuilder` struct.
#[derive(Error, Debug)]
pub enum DocumentBuilderError {
    /// To resolve a document the schema must be set.
    #[error("Schema must be set")]
    SchemaMustBeSet,

    /// An operation with invalid id or previous operations was added to the document.
    #[error("operation {0} cannot be connected to the document graph")]
    InvalidOperationLink(OperationId),

    /// A document can only contain one CREATE operation.
    #[error("multiple CREATE operations found when building operation graph")]
    MultipleCreateOperations,

    /// Handle errors from validating CBOR schemas.
    #[error(transparent)]
    DocumentViewError(#[from] DocumentViewError),

    /// Handle errors when sorting the graph.
    #[error(transparent)]
    GraphSortingError(#[from] crate::graph::error::GraphError),

    /// Handle errors from DocumentReducer.
    #[error(transparent)]
    DocumentReducerError(#[from] DocumentReducerError),
}

/// Error types for methods of `Document` struct.
#[derive(Error, Debug)]
pub enum DocumentError {
    /// Operation passed to commit does not refer to this documents current view.
    #[error("operation {0} does not update the documents current view")]
    PreviousDoesNotMatch(OperationId),

    /// Operation passed to commit does not the same schema as the document.
    #[error("Operation {0} does not match the documents schema")]
    InvalidSchemaId(OperationId),

    /// Cannot perform a commit on a deleted document.
    #[error("Cannot perform a commit on a deleted document")]
    UpdateOnDeleted,

    /// Cannot perform a commit with a create operation.
    #[error("Cannot update an existing document with a create operation")]
    CommitCreate,

    /// Handle errors coming from DocumentView.
    #[error(transparent)]
    DocumentViewError(#[from] DocumentViewError),

    /// Handle errors when sorting the graph.
    #[error(transparent)]
    GraphSortingError(#[from] crate::graph::error::GraphError),
}

/// Error types for methods of `Document` struct.
#[derive(Error, Debug)]
pub enum DocumentReducerError {
    /// The first operation of a document must be a CREATE.
    #[error("The first operation of a document must be a CREATE")]
    FirstOperationNotCreate,

    /// Handle errors from Document.
    #[error(transparent)]
    DocumentError(#[from] DocumentError),
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
