// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

#[derive(Error, Debug)]
pub enum EncodeOperationError {
    #[error("cbor encoder failed {0}")]
    EncoderIOFailed(String),

    /// CBOR encoder could not serialize this value.
    #[error("cbor encoder failed serializing value {0}")]
    EncoderFailed(String),
}

#[derive(Error, Debug)]
pub enum DecodeOperationError {
    #[error("cbor decoder failed {0}")]
    DecoderIOFailed(String),

    #[error("invalid cbor encoding {0}")]
    InvalidCBOREncoding(String),

    #[error("{0}")]
    InvalidEncoding(String),

    #[error("cbor decoder exceeded recursion limit")]
    RecursionLimitExceeded,

    #[error(transparent)]
    EncodeEntryError(#[from] crate::entry::error::EncodeEntryError),

    #[error(transparent)]
    DecodeEntryError(#[from] crate::entry::error::DecodeEntryError),

    #[error(transparent)]
    ValidateOperationError(#[from] ValidateOperationError),

    #[error(transparent)]
    ValidateEntryError(#[from] crate::entry::error::ValidateEntryError),
}

#[derive(Error, Debug)]
pub enum ValidateOperationError {
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
    SchemaValidation(#[from] crate::schema::error::ValidationError),
}

/// Error types for methods of plain fields or operation fields.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum FieldsError {
    /// Detected duplicate field when adding a new one.
    #[error("field '{0}' already exists")]
    FieldDuplicate(String),

    /// Tried to interact with an unknown field.
    #[error("field does not exist")]
    UnknownField,
}

#[derive(Error, Debug)]
pub enum VerifiedOperationError {
    #[error(transparent)]
    ValidateOperationError(#[from] ValidateOperationError),

    #[error(transparent)]
    SchemaValidation(#[from] crate::schema::error::ValidationError),

    #[error(transparent)]
    EncodeEntryError(#[from] crate::entry::error::EncodeEntryError),

    #[error(transparent)]
    DecodeEntryError(#[from] crate::entry::error::DecodeEntryError),

    #[error(transparent)]
    ValidateEntryError(#[from] crate::entry::error::ValidateEntryError),
}
