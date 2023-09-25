// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Errors from `OperationBuilder` struct.
#[derive(Error, Debug)]
pub enum OperationBuilderError {
    /// Handle errors from `operation::validate` module.
    #[error(transparent)]
    ValidateOperationError(#[from] ValidateBodyError),
}

#[derive(Error, Debug)]
pub enum ValidateBodyError {
    /// Claimed schema id did not match given schema.
    #[error("operation schema id not matching with given schema: {0}, expected: {1}")]
    SchemaNotMatching(String, String),

    /// Handle errors from `schema::validate` module.
    #[error(transparent)]
    SchemaValidation(#[from] crate::schema::validate::error::ValidationError),
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
    HashError(#[from] crate::hash_v2::error::HashError),
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

/// Errors from `OperationAction` enum.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum OperationActionError {
    /// Passed unknown operation action value.
    #[error("unknown operation action {0}")]
    UnknownAction(u64),
}
