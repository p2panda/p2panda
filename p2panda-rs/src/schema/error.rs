// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

use crate::operation::OperationValue;

/// Custom error types for schema validation.
#[derive(Error)]
pub enum SchemaError {
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
pub enum SystemSchemaError {
    #[error("invalid field type found for \"{0}\": {1:#?}")]
    InvalidFieldType(String, OperationValue),

    #[error("invalid field \"{1}\" for system schema {0}")]
    InvalidField(String, String),

    #[error("missing field \"{1}\" for system schema {0}")]
    MissingField(String, String),
}

impl std::fmt::Debug for SchemaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            SchemaError::InvalidSchema(_) => write!(f, "InvalidSchema"),
            SchemaError::InvalidCBOR => write!(f, "InvalidCBOR"),
            SchemaError::NoSchema => write!(f, "NoSchema"),
            SchemaError::ParsingError(_) => write!(f, "ParsingError"),
            SchemaError::ValidationError(_) => write!(f, "ValidationError"),
            SchemaError::OperationFieldsError(_) => write!(f, "OperationFieldsError"),
            SchemaError::OperationError(_) => write!(f, "OperationError"),
        }?;

        // We want to format based on `Display` ("{}") instead of `Debug` ("{:?}") to respect line
        // breaks from the displayed error messages.
        f.write_str(format!("({})", self).as_ref())
    }
}
