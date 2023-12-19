// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::operation_v2::body::error::DecodeBodyError;
use crate::operation_v2::error::ValidateOperationError;
use crate::operation_v2::header::error::{DecodeHeaderError, ValidateHeaderError};
use crate::operation_v2::OperationId;
use crate::schema::SchemaId;
use crate::storage_provider::error::OperationStorageError;

/// Error type used in the validation module.
#[derive(thiserror::Error, Debug)]
pub enum ValidationError {
    /// Claimed schema does not match the documents expected schema.
    #[error("Operation {0} claims incorrect schema {1} for document with schema {2}")]
    InvalidClaimedSchema(OperationId, SchemaId, SchemaId),

    /// An operation in the `previous` field was not found in the store.
    #[error("Previous operation {0} not found in store")]
    PreviousOperationNotFound(OperationId),

    /// A document view id was provided which contained operations from different documents.
    #[error("Operations in passed document view id originate from different documents")]
    InvalidDocumentViewId,

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
