// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Custom errors related to `SchemaId`.
#[derive(Error, Debug)]
pub enum SchemaIdError {
    /// Invalid hash in schema id.
    #[error("encountered invalid hash while parsing application schema id: {0}")]
    HashError(#[from] crate::hash::HashError),

    /// Encountered a malformed schema id.
    #[error("malformed application schema id: {0}")]
    MalformedApplicationSchemaId(String),

    /// Application schema ids must start with the schema's name.
    #[error("application schema id is missing a name: {0}")]
    MissingApplicationSchemaName(String),

    /// Invalid system schema id.
    #[error("not a known system schema: {0}")]
    UnknownSystemSchema(String),
}

/// Custom errors related to `Schema`.
#[derive(Error, Debug, Clone, Copy)]
pub enum SchemaError {
    /// Invalid fields in schema.
    #[error("invalid fields found for this schema")]
    InvalidFields,
}

/// Custom error types for field types.
#[derive(Error, Debug)]
pub enum FieldTypeError {
    /// Invalid field type found.
    #[error("invalid field type '{0}'")]
    InvalidFieldType(String),
}
