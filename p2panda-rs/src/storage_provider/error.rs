// SPDX-License-Identifier: AGPL-3.0-or-later

//! Errors from storage provider and associated traits.
use crate::document::error::DocumentBuilderError;
use crate::document::{DocumentId, DocumentViewId};
use crate::hash_v2::error::HashError;
use crate::identity_v2::error::PublicKeyError;
use crate::operation_v2::error::ValidateOperationError;
use crate::operation_v2::OperationId;

/// Data validation errors which can occur in the storage traits.
#[derive(thiserror::Error, Debug)]
pub enum ValidationError {
    /// Error returned from validating p2panda-rs `PublicKey` data types.
    #[error(transparent)]
    AuthorValidation(#[from] PublicKeyError),

    /// Error returned from validating p2panda-rs `Hash` data types.
    #[error(transparent)]
    HashValidation(#[from] HashError),

    /// Error returned from validating p2panda-rs `Operation` data types.
    #[error(transparent)]
    OperationValidation(#[from] ValidateOperationError),

    /// Error returned from validating Bamboo entries.
    #[error(transparent)]
    BambooValidation(#[from] bamboo_rs_core_ed25519_yasmf::verify::Error),
}

/// `OperationStore` errors.
#[derive(thiserror::Error, Debug)]
pub enum OperationStorageError {
    /// Catch all error which implementers can use for passing their own errors up the chain.
    #[error("Error occured in OperationStore: {0}")]
    Custom(String),

    /// A fatal error occured when performing a storage query.
    #[error("A fatal error occured in OperationStore: {0}")]
    FatalStorageError(String),

    /// Error returned when insertion of an operation is not possible due to database constraints.
    #[error("Error occured when inserting an operation with id {0:?} into storage")]
    InsertionError(OperationId),
}

/// `DocumentStore` errors.
#[derive(thiserror::Error, Debug)]
pub enum DocumentStorageError {
    /// Catch all error which implementers can use for passing their own errors up the chain.
    #[error("Error occured in DocumentStore: {0}")]
    Custom(String),

    /// A fatal error occured when performing a storage query.
    #[error("A fatal error occured in DocumentStore: {0}")]
    FatalStorageError(String),

    /// Error which originates in `insert_document_view()` when the insertion fails.
    #[error("Error occured when inserting a document view with id {0:?} into storage")]
    DocumentViewInsertionError(DocumentViewId),

    /// Error which originates in `insert_document()` when the insertion fails.
    #[error("Error occured when inserting a document with id {0:?} into storage")]
    DocumentInsertionError(DocumentId),

    /// Error returned from validating p2panda-rs `Operation` data types.
    #[error(transparent)]
    OperationValidation(#[from] ValidateOperationError),

    /// Error returned from `OperationStorage`.
    #[error(transparent)]
    OperationStorageError(#[from] OperationStorageError),

    /// Error returned from `DocumentBuilder`.
    #[error(transparent)]
    DocumentBuilderError(#[from] DocumentBuilderError),
}
