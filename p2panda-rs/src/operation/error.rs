// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Error types for methods of `Operation` struct.
#[allow(missing_copy_implementations)]
#[derive(Error, Debug)]
pub enum OperationError {
    /// Invalid attempt to create an operation without any fields data.
    #[error("operation fields can not be empty")]
    EmptyFields,
}

/// Error types for methods of `OperationFields` struct.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum OperationFieldsError {
    /// Detected duplicate field when adding a new one.
    #[error("field already exists")]
    FieldDuplicate,

    /// Tried to interact with an unknown field.
    #[error("field does not exist")]
    UnknownField,
}

/// Custom error types for `OperationEncoded`.
#[derive(Error, Debug)]
pub enum OperationEncodedError {
    /// Encoded operation string contains invalid hex characters.
    #[error("invalid hex encoding in operation")]
    InvalidHexEncoding,

    /// Operation can't be deserialized from invalid CBOR encoding.
    #[error("invalid CBOR format")]
    InvalidCBOR,

    /// Handle errors from validating CBOR schemas.
    #[error(transparent)]
    SchemaError(#[from] crate::schema::SchemaError),
}
