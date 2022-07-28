// SPDX-License-Identifier: AGPL-3.0-or-later

//! Error types for encoding, decoding and validating operations with schemas and regarding data
//! types like operation fields, relations or plain operations.
use thiserror::Error;

/// Errors from `OperationBuilder` struct.
#[derive(Error, Debug)]
pub enum OperationBuilderError {
    /// Handle errors from `operation::validate` module.
    #[error(transparent)]
    ValidateOperationError(#[from] ValidateOperationError),
}

/// Errors from `operation::encode` module.
#[derive(Error, Debug)]
pub enum EncodeOperationError {
    /// CBOR encoder failed critically due to an IO issue.
    #[error("cbor encoder failed {0}")]
    EncoderIOFailed(String),

    /// CBOR encoder could not serialize this value.
    #[error("cbor encoder failed serializing value {0}")]
    EncoderFailed(String),
}

/// Errors from `operation::decode` module.
#[derive(Error, Debug)]
pub enum DecodeOperationError {
    /// CBOR decoder failed critically due to an IO issue.
    #[error("cbor decoder failed {0}")]
    DecoderIOFailed(String),

    /// Invalid CBOR encoding detected.
    #[error("invalid cbor encoding at byte {0}")]
    InvalidCBOREncoding(usize),

    /// Invalid p2panda operation encoding detected.
    #[error("{0}")]
    InvalidEncoding(String),

    /// CBOR decoder exceeded maximum recursion limit.
    #[error("cbor decoder exceeded recursion limit")]
    RecursionLimitExceeded,
}

/// Errors from `operation::validate` module.
#[derive(Error, Debug)]
pub enum ValidateOperationError {
    /// Claimed schema id did not match given schema.
    #[error("operation schema id not matching with given schema: {0}, expected: {1}")]
    SchemaNotMatching(String, String),

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

    /// Handle errors from `schema::validate` module.
    #[error(transparent)]
    SchemaValidation(#[from] crate::next::schema::error::ValidationError),
}

/// Error types for methods of plain fields or operation fields.
#[derive(Error, Debug)]
pub enum FieldsError {
    /// Detected duplicate field when adding a new one.
    #[error("field '{0}' already exists")]
    FieldDuplicate(String),

    /// Tried to interact with an unknown field.
    #[error("field does not exist")]
    UnknownField,
}

/// Errors from converting to a `VerifiedOperation` in `operation:validate` module.
#[derive(Error, Debug)]
pub enum VerifiedOperationError {
    /// Handle errors from `operation::validate` module.
    #[error(transparent)]
    ValidateOperationError(#[from] ValidateOperationError),

    /// Handle errors from `entry::validate` module.
    #[error(transparent)]
    ValidateEntryError(#[from] crate::next::entry::error::ValidateEntryError),
}
