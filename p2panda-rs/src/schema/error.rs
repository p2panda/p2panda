// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

use crate::operation::OperationValue;

/// Custom error types for schema validation.
#[derive(Error)]
pub enum SchemaValidationError {
    /// Operation contains invalid fields.
    // Note: We pretty-print the vector of error strings to get line breaks
    #[error("invalid operation schema: {0:#?}")]
    InvalidSchema(Vec<String>),

    /// Operation can't be deserialised from invalid CBOR encoding.
    #[error("invalid CBOR format")]
    InvalidCBOR,

    /// There is no schema set.
    #[error("no CDDL schema present")]
    NoSchema,

    /// Error while parsing CDDL.
    #[error("error while parsing CDDL: {0}")]
    ParsingError(String),

    /// Operation contains invalid values.
    #[error("invalid operation values")]
    ValidationError(String),

    /// `OperationFields` error.
    #[error("error while adding operation fields")]
    OperationFieldsError(#[from] crate::operation::OperationFieldsError),

    /// `Operation` error.
    #[error("error while creating operation")]
    OperationError(#[from] crate::operation::OperationError),
}

/// Custom error types for schema validation.
#[derive(Error, Debug)]
pub enum SchemaIdError {
    /// `OperationFields` error.
    #[error("invalid hash string")]
    HashError(#[from] crate::hash::HashError),
}

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

impl std::fmt::Debug for SchemaValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            SchemaValidationError::InvalidSchema(_) => write!(f, "InvalidSchema"),
            SchemaValidationError::InvalidCBOR => write!(f, "InvalidCBOR"),
            SchemaValidationError::NoSchema => write!(f, "NoSchema"),
            SchemaValidationError::ParsingError(_) => write!(f, "ParsingError"),
            SchemaValidationError::ValidationError(_) => write!(f, "ValidationError"),
            SchemaValidationError::OperationFieldsError(_) => write!(f, "OperationFieldsError"),
            SchemaValidationError::OperationError(_) => write!(f, "OperationError"),
        }?;

        // We want to format based on `Display` ("{}") instead of `Debug` ("{:?}") to respect line
        // breaks from the displayed error messages.
        f.write_str(format!("({})", self).as_ref())
    }
}
