// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Errors from `OperationBuilder` struct.
#[derive(Error, Debug)]
pub enum OperationBuilderError {
    /// Handle errors from `operation::validate` module.
    #[error(transparent)]
    ValidateOperationError(#[from] ValidateOperationError),

    #[error(transparent)]
    EncodeBody(#[from] crate::operation::body::error::EncodeBodyError),

    #[error(transparent)]
    EncodeHeader(#[from] crate::operation::header::error::EncodeHeaderError),
}

#[derive(thiserror::Error, Debug)]
pub enum ValidateHeaderExtensionsError {
    /// Expect `timestamp` on all operations.
    #[error("expected 'timestamp' in operation header")]
    ExpectedTimestamp,

    /// Expect `depth` on all operations.
    #[error("expected 'depth' in operation header")]
    ExpectedDepth,

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

    #[error(transparent)]
    HeaderValidation(#[from] crate::operation::header::error::ValidateHeaderError),
}

#[derive(Error, Debug)]
pub enum ValidateOperationError {
    /// Expected `fields` in CREATE or UPDATE operation.
    #[error("expected 'fields' in CREATE or UPDATE operation")]
    ExpectedFields,

    /// Unexpected `fields` in DELETE operation.
    #[error("unexpected 'fields' in DELETE operation")]
    UnexpectedFields,

    #[error(transparent)]
    HeaderValidation(#[from] crate::operation::header::error::ValidateHeaderError),

    #[error(transparent)]
    HeaderExtensionValidation(#[from] crate::operation::error::ValidateHeaderExtensionsError),
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
