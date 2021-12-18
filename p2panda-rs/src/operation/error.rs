// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Error types for methods of `Operation` struct.
#[allow(missing_copy_implementations)]
#[derive(Error, Debug)]
pub enum OperationError {
    /// Invalid attempt to create an operation without any fields data.
    #[error("operation fields can not be empty")]
    EmptyFields,

    /// Invalid attempt to create an operation without any previous operations data.
    #[error("previous_operations field can not be empty")]
    EmptyPreviousOperations,

    /// Invalid attempt to create an operation with previous operations data.
    #[error("previous_operations field should be empty")]
    ExistingPreviousOperations,
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

/// Error types for methods of `OperationWithMeta` struct.
#[derive(Error, Debug)]
pub enum OperationWithMetaError {
    /// Invalid attempt to create an operation with meta with invalid encoded entry.
    #[error(transparent)]
    EntrySignedError(#[from] crate::entry::EntrySignedError),

    /// Invalid attempt to create an operation with meta with invalid encoded operation.
    #[error(transparent)]
    OperationEncodedError(#[from] OperationEncodedError),

    /// Operation with meta contans invalid operation.
    #[error(transparent)]
    OperationError(#[from] OperationError),

    /// Operation with meta contans invalid author.
    #[error(transparent)]
    AuthorError(#[from] crate::identity::AuthorError),

    /// Operation with meta contans invalid operation id hash.
    #[error(transparent)]
    HashError(#[from] crate::hash::HashError),
}
