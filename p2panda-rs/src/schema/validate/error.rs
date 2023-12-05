// SPDX-License-Identifier: AGPL-3.0-or-later

//! Error types for validating operation fields against schemas.
use thiserror::Error;

use crate::operation_v2::body::error::PlainValueError;

/// Custom error types for validating raw operations with schemas.
#[derive(Error, Debug)]
pub enum ValidationError {
    /// Field with this name is required by schema.
    #[error("missing required field: '{0}' of type {1}")]
    MissingField(String, String),

    /// One or more fields which do not belong to the schema.
    #[error("unexpected fields found: {0}")]
    UnexpectedFields(String),

    /// Raw operation field did not match schema.
    #[error("field '{0}' does not match schema: {1}")]
    InvalidField(String, String),

    /// Field type and schema do not match.
    #[error("expected field name '{1}'")]
    InvalidName(String, String),

    /// Field type and schema do not match.
    #[error("invalid field type '{0}', expected '{1}'")]
    InvalidType(String, String),

    /// Field value is not correctly formatted.
    #[error("{0}")]
    InvalidValue(String),

    /// Field value is not in canonic format.
    #[error("non-canonic document view id, {0}")]
    InvalidDocumentViewId(String),

    /// Error from validating system schema: `schema_definition_v1`.
    #[error("invalid 'schema_definition_v1' operation: {0}")]
    InvalidSchemaDefinition(#[from] SchemaDefinitionError),

    /// Error from validating system schema: `schema_field_definition_v1`.
    #[error("invalid 'schema_field_definition_v1' operation: {0}")]
    InvalidSchemaFieldDefinition(#[from] SchemaFieldDefinitionError),

    /// Error from conversion of PlainValues.
    #[error(transparent)]
    NotStringValue(#[from] PlainValueError),

    /// Error from validating system schema: `blob_v1`.
    #[error("invalid 'blob_v1' operation: {0}")]
    InvalidBlob(#[from] BlobError),

    /// Error from validating system schema: `blob_piece_v1`.
    #[error("invalid 'blob_piece_v1' operation: {0}")]
    InvalidBlobPiece(#[from] BlobPieceError),
}

/// Custom error types for validating operations against `schema_field_definition_v1` schema.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum SchemaFieldDefinitionError {
    /// "name" is not correctly formatted as per specification.
    #[error("'name' field in schema field definitions is wrongly formatted")]
    NameInvalid,

    /// "type" is not correctly formatted as per specification.
    #[error("'type' field in schema field definitions is wrongly formatted")]
    TypeInvalid,

    /// Error from conversion of PlainValues.
    #[error(transparent)]
    NotStringValue(#[from] PlainValueError),
}

/// Custom error types for validating operations against `schema_field_definition_v1` schema.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum SchemaDefinitionError {
    /// "name" is not correctly formatted as per specification.
    #[error("'name' field in schema field definitions is wrongly formatted")]
    NameInvalid,

    /// "description" is not correctly formatted as per specification.
    #[error("'description' field in schema field definitions is wrongly formatted")]
    DescriptionInvalid,

    /// "fields" is not correctly formatted as per specification.
    #[error("'fields' field in schema field definitions is wrongly formatted")]
    FieldsInvalid,

    /// Error from conversion of PlainValues.
    #[error(transparent)]
    NotStringValue(#[from] PlainValueError),
}

/// Custom error types for validating operations against `blob_v1` schema.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum BlobError {
    /// "mime_type" is not correctly formatted as per specification.
    #[error("'mime_type' field in blob is wrongly formatted")]
    MimeTypeInvalid,

    /// "pieces" can not be empty as per specification.
    #[error("'pieces' field can not be empty")]
    PiecesEmpty,
}

/// Custom error types for validating operations against `blob_piece_v1` schema.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum BlobPieceError {
    /// "data" is greater than the maximum allowed as per specification.
    #[error("'data' field in blob is over maximum allowed length")]
    DataInvalid,
}
