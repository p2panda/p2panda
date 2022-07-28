// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

use crate::next::schema::SchemaId;

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
    HashError(#[from] crate::next::hash::error::HashError),

    /// Handle errors from validating document view ids.
    #[error(transparent)]
    DocumentViewIdError(#[from] crate::next::document::error::DocumentViewIdError),

    /// Handle errors from validating operation ids.
    #[error(transparent)]
    OperationIdError(#[from] crate::next::operation::error::OperationIdError),
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
    #[error("invalid value format, {0}")]
    InvalidValue(String),

    /// Field value is not in canonic format.
    #[error("non-canonic document view id, {0}")]
    InvalidDocumentViewId(String),
}
