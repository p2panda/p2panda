// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::document::DocumentId;
use crate::hash::Hash;
use crate::identity::PublicKey;
use crate::operation::header::SeqNum;
use crate::operation::OperationId;
use crate::schema::SchemaId;

#[derive(thiserror::Error, Debug)]
pub enum ValidatePlainOperationError {
    /// Claimed schema id did not match given schema.
    #[error("operation schema id not matching with given schema: {0}, expected: {1}")]
    SchemaNotMatching(String, String),

    /// Expected `fields` in CREATE or UPDATE operation.
    #[error("expected 'fields' in CREATE or UPDATE operation")]
    ExpectedFields,

    /// Unexpected `fields` in DELETE operation.
    #[error("unexpected 'fields' in DELETE operation")]
    UnexpectedFields,

    #[error(transparent)]
    ValidateFieldsError(#[from] crate::schema::validate::error::ValidationError),
}

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

    /// A backlink which does not match the latest operation for for the public key and document
    /// was found.
    #[error("Backlink {0} does not match latest operation for public key {1} and document {2}, expected: {3}")]
    IncorrectBacklink(Hash, PublicKey, DocumentId, Hash),

    /// An operation in the `previous` field was not found in the store.
    #[error("Previous operation {0} not found in store")]
    PreviousOperationNotFound(OperationId),

    /// An operation was found in `previous` with a mismatching document id.
    #[error("Operation {0} contains a previous operation with document id {1}, expected: {2}")]
    MismatchingDocumentIdInPrevious(OperationId, DocumentId, DocumentId),

    /// An operation was found in `previous` with a mismatching schema id.
    #[error("Operation {0} contains a previous operation with schema id {1}, expected: {2}")]
    MismathingSchemaInPrevious(OperationId, SchemaId, SchemaId),

    /// An operation was found with a timestamp not greater than the one in it's previous operations.
    #[error(
        "Operation {0} contains a timestamp {1} which is not greater than those found in previous"
    )]
    TimestampLessThanPrevious(OperationId, u64),

    /// An operation was found with a timestamp not greater than the one in it's backlink.
    #[error(
        "Operation {0} contains a timestamp {1} which is not greater than it's backlink timestamp"
    )]
    TimestampLessThanBacklink(OperationId, u64),

    /// An operation was found with a depth not greater than the one in it's previous operations.
    #[error(
        "Operation {0} contains a depth {1} which is not greater than those found in previous"
    )]
    DepthLessThanPrevious(OperationId, SeqNum),

    /// An operation was found with a depth not greater than the one in it's backlink.
    #[error("Operation {0} contains a depth {1} which is not greater than it's backlink depth")]
    DepthLessThanBacklink(OperationId, SeqNum),

    /// Error coming from the operation store.
    #[error(transparent)]
    OperationStoreError(#[from] crate::storage_provider::error::OperationStorageError),
}

/// Error type used in the domain module.
#[derive(thiserror::Error, Debug)]
pub enum DomainError {
    /// Validation errors.
    #[error(transparent)]
    ValidationError(#[from] ValidationError),

    /// Error coming from the operation store.
    #[error(transparent)]
    OperationStoreError(#[from] crate::storage_provider::error::OperationStorageError),

    /// Error occurring when decoding header.
    #[error(transparent)]
    DecodeHeaderError(#[from] crate::operation::header::error::DecodeHeaderError),

    /// Error occurring when decoding body.
    #[error(transparent)]
    DecodeBodyError(#[from] crate::operation::body::error::DecodeBodyError),

    /// Error occurring when validating operations.
    #[error(transparent)]
    ValidateOperationError(#[from] crate::operation::error::ValidateOperationError),

    /// Error occurring when validating plain operations.
    #[error(transparent)]
    ValidatePlainOperationError(#[from] ValidatePlainOperationError),

    /// Error occurring when validating headers.
    #[error(transparent)]
    ValidateHeaderError(#[from] crate::operation::header::error::ValidateHeaderError),
}
