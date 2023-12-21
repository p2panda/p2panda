// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::document::DocumentId;
use crate::operation::body::error::DecodeBodyError;
use crate::operation::error::ValidateOperationError;
use crate::operation::header::error::{DecodeHeaderError, ValidateHeaderError};
use crate::operation::OperationId;
use crate::schema::SchemaId;
use crate::storage_provider::error::OperationStorageError;

/// Error type used in the validation module.
#[derive(thiserror::Error, Debug)]
pub enum ValidationError {
    /// Claimed schema does not match the documents expected schema.
    #[error("Operation {0} claims incorrect schema {1}")]
    InvalidClaimedSchema(OperationId, SchemaId),

    /// An operation in the `previous` field was not found in the store.
    #[error("Previous operation {0} not found in store")]
    PreviousOperationNotFound(OperationId),

    /// A document view id was provided which contained operations from different documents.
    #[error("Operations in passed document view id originate from different documents")]
    InvalidDocumentViewId,

    /// An operation was found with an incorrect document id which.
    #[error("Operation {0} document id {1} does not match expected document id")]
    IncorrectDocumentId(OperationId, DocumentId),

    /// An operation was found with a timestamp not greater than the one in it's previous operations.
    #[error("Operation {0} contains a timestamp {1} not greater than those found in previous")]
    InvalidTimestamp(OperationId, u64),

    /// Error coming from the operation store.
    #[error(transparent)]
    OperationStoreError(#[from] OperationStorageError),
}

/// Error type used in the domain module.
#[derive(thiserror::Error, Debug)]
pub enum DomainError {
    /// Validation errors.
    #[error(transparent)]
    ValidationError(#[from] ValidationError),

    /// Error coming from the operation store.
    #[error(transparent)]
    OperationStoreError(#[from] OperationStorageError),

    /// Error occurring when decoding header.
    #[error(transparent)]
    DecodeHeaderError(#[from] DecodeHeaderError),

    /// Error occurring when decoding body.
    #[error(transparent)]
    DecodeBodyError(#[from] DecodeBodyError),

    /// Error occurring when validating operations.
    #[error(transparent)]
    ValidateOperationError(#[from] ValidateOperationError),

    /// Error occurring when validating headers.
    #[error(transparent)]
    ValidateHeaderError(#[from] ValidateHeaderError),
}
