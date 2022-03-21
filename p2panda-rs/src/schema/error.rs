// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Custom errors related to `SchemaId`.
#[derive(Error, Debug)]
pub enum SchemaIdError {
    /// Handle errors from validating operation id hashes.
    #[error(transparent)]
    DocumentViewIdError(#[from] crate::document::DocumentViewIdError),
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
