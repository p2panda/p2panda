// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Error types for methods of `Message` struct.
#[allow(missing_copy_implementations)]
#[derive(Error, Debug)]
pub enum MessageError {
    /// Invalid attempt to create a message without any fields data.
    #[error("message fields can not be empty")]
    EmptyFields,
}

/// Error types for methods of `MessageFields` struct.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum MessageFieldsError {
    /// Detected duplicate field when adding a new one.
    #[error("field already exists")]
    FieldDuplicate,

    /// Tried to interact with an unknown field.
    #[error("field does not exist")]
    UnknownField,
}

/// Custom error types for `MessageEncoded`.
#[derive(Error, Debug)]
pub enum MessageEncodedError {
    /// Encoded message string contains invalid hex characters.
    #[error("invalid hex encoding in message")]
    InvalidHexEncoding,

    /// Message can't be deserialized from invalid CBOR encoding.
    #[error("invalid CBOR format")]
    InvalidCBOR,

    /// Handle errors from validating CBOR schemas.
    #[error(transparent)]
    SchemaError(#[from] crate::schema::error::SchemaError),
}
