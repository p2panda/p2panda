// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Custom errors related to `SchemaId`.
#[derive(Error, Debug)]
pub enum SchemaIdError {
    /// Invalid hash in schema id.
    #[error("encountered invalid hash while parsing application schema id: {0}")]
    ParsingApplicationSchema(#[from] crate::hash::HashError),

    /// Invalid application schema id.
    #[error("invalid application schema id: {0}")]
    InvalidApplicationSchemaId(String),

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
