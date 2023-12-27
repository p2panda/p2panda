// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Errors from `OperationBuilder` struct.
#[derive(Error, Debug)]
pub enum OperationBuilderError {
    #[error("required field 'document_id' missing from header extensions")]
    MissingDocumentId,

    /// Handle errors from `operation::validate` module.
    #[error(transparent)]
    ValidateOperationError(#[from] ValidateOperationError),

    #[error(transparent)]
    EncodeBody(#[from] crate::operation::body::error::EncodeBodyError),

    #[error(transparent)]
    EncodeHeader(#[from] crate::operation::header::error::EncodeHeaderError),

    #[error(transparent)]
    SystemTime(#[from] std::time::SystemTimeError),
}

#[derive(Error, Debug)]
pub enum ValidateOperationError {
    /// Claimed schema id did not match given schema.
    #[error("operation schema id not matching with given schema: {0}, expected: {1}")]
    SchemaNotMatching(String, String),

    /// Claimed document id did not match CREATE operation id.
    #[error("document id of CREATE operation must match operation id: {0}, expected: {1}")]
    IncorrectDocumentId(String, String),

    /// Expect `document_id` on all operations.
    #[error("expected 'document_id' in operation header")]
    ExpectedDocumentId,

    /// Expect `document_id` on all operations.
    #[error("unexpected 'document_id' in CREATE operation header")]
    UnexpectedDocumentId,

    /// Expect `timestamp` on all operations.
    #[error("expected 'timestamp' in operation header")]
    ExpectedTimestamp,

    /// Expect `depth` on all operations.
    #[error("expected 'depth' in operation header")]
    ExpectedDepth,

    /// Expected `fields` in CREATE or UPDATE operation.
    #[error("expected 'fields' in CREATE or UPDATE operation")]
    ExpectedFields,

    /// Unexpected `fields` in DELETE operation.
    #[error("unexpected 'fields' in DELETE operation")]
    UnexpectedFields,

    /// Expected `previous` in UPDATE or DELETE operation.
    #[error("expected 'previous' in UPDATE or DELETE operation")]
    ExpectedPreviousOperations,

    /// Unexpected `previous` in CREATE operation.
    #[error("unexpected 'previous' in CREATE operation")]
    UnexpectedPreviousOperations,

    /// Expected `backlink` in UPDATE or DELETE operation.
    #[error("expected 'backlink' in UPDATE or DELETE operation")]
    ExpectedBacklink,

    /// Unexpected `backlink` in CREATE operation.
    #[error("unexpected 'backlink' in CREATE operation")]
    UnexpectedBacklink,

    /// Expected 'depth' to be 0 for CREATE operation.
    #[error("expected 'depth' to be 0 for CREATE operation")]
    ExpectedZeroDepth,

    /// Expected 'depth' to be to be non-zero u64 for UPDATE and DELETE operations.
    #[error("expected 'depth' to be non-zero u64 for UPDATE and DELETE operations")]
    ExpectedNonZeroDepth,

    /// Handle errors from `schema::validate` module.
    #[error(transparent)]
    SchemaValidation(#[from] crate::schema::validate::error::ValidationError),

    #[error(transparent)]
    HeaderValidation(#[from] crate::operation::header::error::ValidateHeaderError),

    #[error(transparent)]
    EncodeHeader(#[from] crate::operation::header::error::EncodeHeaderError),
}

/// Error types for methods of plain fields or operation fields.
#[derive(Error, Debug)]
pub enum FieldsError {
    /// Detected duplicate field when adding a new one.
    #[error("field '{0}' already exists")]
    FieldDuplicate(String),

    /// Tried to interact with an unknown field.
    #[error("field does not exist")]
    UnknownField,
}

/// Errors from `OperationId` struct.
#[derive(Error, Debug)]
pub enum OperationIdError {
    /// Handle errors from `Hash` struct.
    #[error(transparent)]
    HashError(#[from] crate::hash::error::HashError),
}

/// Errors from `Relation` struct.
#[derive(Error, Debug)]
pub enum RelationError {
    /// Handle errors from `DocumentId` struct.
    #[error(transparent)]
    DocumentIdError(#[from] crate::document::error::DocumentIdError),
}

/// Errors from `PinnedRelation` struct.
#[derive(Error, Debug)]
pub enum PinnedRelationError {
    /// Handle errors from `DocumentViewId` struct.
    #[error(transparent)]
    DocumentViewIdError(#[from] crate::document::error::DocumentViewIdError),
}

/// Errors from `RelationList` struct.
#[derive(Error, Debug)]
pub enum RelationListError {
    /// Handle errors from `DocumentId` struct.
    #[error(transparent)]
    DocumentIdError(#[from] crate::document::error::DocumentIdError),
}

/// Errors from `PinnedRelationList` struct.
#[derive(Error, Debug)]
pub enum PinnedRelationListError {
    /// Handle errors from `DocumentViewId` struct.
    #[error(transparent)]
    DocumentViewIdError(#[from] crate::document::error::DocumentViewIdError),
}
