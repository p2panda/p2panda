// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::document::DocumentId;
use crate::hash::Hash;
use crate::identity::PublicKey;
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

    /// An existing document log was found for this public key document id combination.
    #[error("Existing document log found for public key {0} and document {1}")]
    UnexpectedDocumentLog(PublicKey, DocumentId),

    /// An existing document log was not found for this public key document id combination.
    #[error("Document log not found for public key {0} and document {1}")]
    ExpectedDocumentLog(PublicKey, DocumentId),

    /// A document view id was provided which contained operations from different documents.
    #[error("Backlink {0} does not match latest operation for public key {1} and document {2}, expected: {3}")]
    IncorrectBacklink(Hash, PublicKey, DocumentId, Hash),

    /// An operation in the `previous` field was not found in the store.
    #[error("Previous operation {0} not found in store")]
    PreviousOperationNotFound(OperationId),

    /// A document view id was provided which contained operations from different documents.
    #[error("Operations in passed document view id originate from different documents")]
    InvalidDocumentViewId,

    /// An operation was found in `previous` with a mismatching document id.
    #[error("Previous operation {0} has document id {1}, expected: {2}")]
    MismathingDocumentIdInPrevious(OperationId, DocumentId, DocumentId),

    /// An operation was found in `previous` with a mismatching schema id.
    #[error("Previous operation {0} has schema id {1}, expected: {2}")]
    MismathingSchemaInPrevious(OperationId, SchemaId, SchemaId),

    /// An operation was found with a timestamp not greater than the one in it's previous operations.
    #[error(
        "Operation {0} contains a timestamp {1} which is not greater than those found in previous"
    )]
    TimestampLessThanPrevious(OperationId, u128),

    /// An operation was found with a timestamp not greater than the one in it's backlink.
    #[error(
        "Operation {0} contains a timestamp {1} which is not greater than it's backlink timestamp"
    )]
    TimestampLessThanBacklink(OperationId, u128),

    /// An operation was found with a depth not greater than the one in it's previous operations.
    #[error(
        "Operation {0} contains a depth {1} which is not greater than those found in previous"
    )]
    DepthLessThanPrevious(OperationId, u64),

    /// An operation was found with a depth not greater than the one in it's backlink.
    #[error("Operation {0} contains a depth {1} which is not greater than it's backlink depth")]
    DepthLessThanBacklink(OperationId, u64),

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
