// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

use crate::document::DocumentViewValue;

/// Custom error types for system schema views.
#[derive(Error, Debug)]
pub enum SystemSchemaError {
    /// Passed field type does not match the expected type.
    #[error("invalid field \"{0}\" with value {1:#?}")]
    InvalidField(String, DocumentViewValue),

    /// Missing expected field.
    #[error("missing field \"{0}\"")]
    MissingField(String),

    /// Additional field found.
    #[error("additional field found")]
    AdditionalFields,

    /// Invalid field type found.
    #[error("invalid field type")]
    InvalidFieldType(#[from] crate::schema::error::FieldTypeError),
}
