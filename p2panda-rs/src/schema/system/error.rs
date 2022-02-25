// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

use crate::operation::OperationValue;

/// Custom error types for system schema views.
#[derive(Error, Debug)]
pub enum SystemSchemaError {
    /// Passed field type does not match the expected type.
    #[error("invalid field \"{0}\" with value {1:#?}")]
    InvalidField(String, OperationValue),

    /// Missing expected field.
    #[error("missing field \"{0}\"")]
    MissingField(String),

    /// Too many fields passed.
    #[error("too many fields")]
    TooManyFields,

    /// Too few fields passed.
    #[error("too few fields")]
    TooFewFields,

    /// Invalid field type found.
    #[error("invalid field type")]
    InvalidFieldType,
}
