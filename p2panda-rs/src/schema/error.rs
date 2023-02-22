// SPDX-License-Identifier: AGPL-3.0-or-later

//! Error types for creating schema instances and schema ids.
use thiserror::Error;

use crate::schema::SchemaId;

/// Custom errors related to `SchemaName`.
#[derive(Clone, Error, Debug)]
pub enum SchemaNameError {
    /// Encountered a malformed schema name.
    #[error("Schema name contains too many or invalid characters")]
    MalformedSchemaName,
}

impl Copy for SchemaNameError {}

/// Custom errors related to `SchemaDescription`.
#[derive(Clone, Error, Debug)]
pub enum SchemaDescriptionError {
    /// Encountered a too long schema description.
    #[error("Schema description contains more than 256 characters")]
    TooLongSchemaDescription,
}

impl Copy for SchemaDescriptionError {}

/// Custom errors related to `SchemaFields`.
#[derive(Clone, Error, Debug)]
pub enum SchemaFieldError {
    /// Encountered an invalid name in a schema field key.
    #[error("Schema field key has an invalid name")]
    MalformedSchemaFieldName,

    /// Maximum number of schema fields has been reached no more can be added.
    #[error("Maximum number of schema fields (1024) reached")]
    MaxSchemaFieldsReached,

    /// Schema fields length must be at least 1.
    #[error("Schema fields length must be at least 1")]
    ZeroFields,

    /// Schema fields cannot contain duplicate fields.
    #[error("Schema fields instantiated with duplicate fields")]
    DuplicateFields,
}

impl Copy for SchemaFieldError {}

/// Custom errors related to `SchemaId`.
#[derive(Error, Debug)]
pub enum SchemaIdError {
    /// Encountered a malformed schema id.
    #[error("malformed schema id `{0}`: {1}")]
    MalformedSchemaId(String, String),

    /// Application schema ids must start with the schema's name.
    #[error("application schema id is missing a name: {0}")]
    MissingApplicationSchemaName(String),

    /// Invalid system schema id.
    #[error("unsupported system schema: {0}")]
    UnknownSystemSchema(String),

    /// Invalid hash in schema id.
    #[error("encountered invalid hash while parsing application schema id: {0}")]
    HashError(#[from] crate::hash::error::HashError),

    /// Handle errors from validating document view ids.
    #[error("encountered invalid document view id while parsing application schema id: {0}")]
    DocumentViewIdError(#[from] crate::document::error::DocumentViewIdError),

    /// Handle errors from validating operation ids.
    #[error("encountered invalid hash while parsing application schema id: {0}")]
    OperationIdError(#[from] crate::operation::error::OperationIdError),
}

/// Custom errors related to `Schema`.
#[derive(Error, Debug)]
pub enum SchemaError {
    /// Invalid fields in schema.
    #[error("invalid fields found for this schema")]
    InvalidFields,

    /// Use static definitions of system schemas instead of defining them dynamically.
    #[error("dynamic redefinition of system schema {0}, use `Schema::get_system` instead")]
    DynamicSystemSchema(SchemaId),

    /// Schemas must have valid schema ids.
    #[error(transparent)]
    SchemaIdError(#[from] SchemaIdError),

    /// Schemas must have valid schema names.
    #[error(transparent)]
    SchemaNameError(#[from] SchemaNameError),

    /// Schemas must have valid schema descriptions.
    #[error(transparent)]
    SchemaDescriptionError(#[from] SchemaDescriptionError),

    /// Schemas must have valid schema fields.
    #[error(transparent)]
    SchemaFieldsError(#[from] SchemaFieldError),
}

/// Custom error types for field types.
#[derive(Error, Debug)]
pub enum FieldTypeError {
    /// Invalid field type found.
    #[error("invalid field type '{0}'")]
    InvalidFieldType(String),

    /// Schema ids referenced by relation field types need to be valid.
    #[error(transparent)]
    RelationSchemaReference(#[from] SchemaIdError),
}
