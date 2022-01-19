// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Error types for methods of `DocumentBuilder` struct.
#[allow(missing_copy_implementations)]
#[derive(Error, Debug)]
pub enum DocumentBuilderError {
    /// No create operation found.
    #[error("Every document must contain one create operation")]
    NoCreateOperation,

    /// A document can only have one create operation.
    #[error("Multiple create operations found")]
    MoreThanOneCreateOperation,

    /// Handle errors from validating CBOR schemas.
    #[error(transparent)]
    SchemaError(#[from] crate::schema::SchemaError),

    /// All operation in a document must follow the same schema.
    #[error("Operation {0} contains a schema not mathing this document.")]
    OperationSchemaNotMatching(String),

    /// An operation with invalid id or previous operations was added to the document.
    #[error("Operation {0} cannot be connected to the document graph")]
    InvalidOperationLink(String),
}

/// Error types for methods of `Document` struct.
#[allow(missing_copy_implementations)]
#[derive(Error, Debug)]
pub enum DocumentError {
    /// Handle errors when sorting the graph.
    #[error(transparent)]
    GraphSortingError(#[from] crate::materialiser::GraphError),

    /// Handle errors from validating CBOR schemas.
    #[error(transparent)]
    InstanceError(#[from] crate::instance::InstanceError),
}
