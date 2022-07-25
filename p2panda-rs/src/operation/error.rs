// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Error types for methods of `Operation` struct.
#[allow(missing_copy_implementations)]
#[derive(Error, Debug)]
pub enum OperationError {
    /// Invalid attempt to create an operation without any fields data.
    #[error("operation fields can not be empty")]
    EmptyFields,

    /// Invalid attempt to create a delete operation with fields.
    #[error("DELETE operation must not have fields")]
    DeleteWithFields,

    /// Invalid attempt to create an operation without any previous operations data.
    #[error("previous_operations field can not be empty")]
    EmptyPreviousOperations,

    /// Invalid attempt to create an operation with previous operations data.
    #[error("previous_operations field should be empty")]
    ExistingPreviousOperations,

    /// Invalid hash found.
    #[error(transparent)]
    HashError(#[from] crate::hash::HashError),
}

/// Error types for `RawOperation` struct and methods related to it.
#[derive(Error, Debug)]
pub enum RawOperationError {
    /// Could not encode to CBOR due to internal error.
    #[error("{0}")]
    EncoderFailed(String),

    /// Could not decode CBOR due to internal error.
    #[error("{0}")]
    DecoderFailed(String),

    /// Could not decode CBOR of raw operation.
    #[error("{0}")]
    InvalidCBOREncoding(String),

    /// Could not decode p2panda data of raw operation.
    #[error("{0}")]
    InvalidEncoding(String),

    /// Expected `fields` in CREATE or UPDATE operation.
    #[error("expected 'fields' in CREATE or UPDATE operation")]
    ExpectedFields,

    /// Unexpected `fields` in DELETE operation.
    #[error("unexpected 'fields' in DELETE operation")]
    UnexpectedFields,

    /// Expected `previous_operations` in UPDATE or DELETE operation.
    #[error("expected 'previous_operations' in UPDATE or DELETE operation")]
    ExpectedPreviousOperations,

    /// Unexpected `previous_operations` in CREATE operation.
    #[error("unexpected 'previous_operations' in CREATE operation")]
    UnexpectedPreviousOperations,

    /// Detected duplicate field name in operation.
    #[error("found duplicate field '{0}'")]
    DuplicateFieldName(String),

    /// Operation did not match given schema.
    #[error(transparent)]
    SchemaValidation(#[from] crate::schema::ValidationError),
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

/// Custom error types for `EncodedOperation`.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum EncodedOperationError {
    /// Encoded operation string contains invalid hex characters.
    #[error("invalid hex encoding in operation")]
    InvalidHexEncoding(#[from] hex::FromHexError),

    /// Something went wrong with encoding or decoding from / to raw operation.
    #[error(transparent)]
    RawOperationError(#[from] RawOperationError),
}

/// Error types for methods of `VerifiedOperation` struct.
#[derive(Error, Debug)]
pub enum VerifiedOperationError {
    /// Invalid encoded entry found.
    #[error(transparent)]
    EntrySignedError(#[from] crate::entry::EntrySignedError),

    /// Encoded operation data is invalid.
    #[error(transparent)]
    EncodedOperationError(#[from] EncodedOperationError),

    /// Invalid operation found.
    #[error(transparent)]
    OperationError(#[from] OperationError),

    /// Invalid author found.
    #[error(transparent)]
    AuthorError(#[from] crate::identity::AuthorError),

    /// Invalid operation id hash found.
    #[error(transparent)]
    HashError(#[from] crate::hash::HashError),
}
