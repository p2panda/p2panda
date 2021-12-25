// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Custom error types for schema validation.
#[derive(Error, Debug)]
pub enum SchemaError {
    /// Operation contains invalid fields.
    #[error("invalid operation schema: {0}")]
    InvalidSchema(String),

    /// Operation can't be deserialized from invalid CBOR encoding.
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
