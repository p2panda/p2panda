// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

#[derive(Error, Debug)]
pub enum EncodeHeaderError {
    /// CBOR encoder failed critically due to an IO issue.
    #[error("cbor encoder failed {0}")]
    EncoderIOFailed(String),

    /// CBOR encoder could not serialize this value.
    #[error("cbor encoder failed serializing value {0}")]
    EncoderFailed(String),

    #[error(transparent)]
    ValidateHeaderError(#[from] ValidateHeaderError),
}

#[derive(Error, Debug)]
pub enum DecodeHeaderError {
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

    #[error(transparent)]
    ValidateHeaderError(#[from] ValidateHeaderError),
}

#[derive(Error, Debug)]
pub enum ValidateHeaderError {
    /// Payload needs to match claimed hash in header.
    #[error("body doesn't match claimed payload hash of header")]
    PayloadHashMismatch,

    /// Payload needs to match claimed size in header.
    #[error("body doesn't match claimed payload size in header")]
    PayloadSizeMismatch,

    /// Header without document ids are considered CREATE and should not contain a backlink.
    #[error("unexpected backlink found on CREATE header")]
    CreateUnexpectedBacklink,

    /// Header without document ids are considered CREATE and should not contain previous.
    #[error("unexpected previous found on CREATE header")]
    CreateUnexpectedPrevious,

    /// DELETE header expected to contain a document id.
    #[error("expected document id on DELETE header")]
    DeleteExpectedDocumentId,

    /// Could not verify authorship of operation.
    #[error("signature invalid")]
    KeyPairError(#[from] crate::identity::error::KeyPairError),
}

#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum HeaderActionError {
    /// Passed unknown operation action value.
    #[error("unknown operation action {0}")]
    UnknownAction(u64),
}

/// Errors from `SeqNum` struct.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum SeqNumError {
    /// Sequence numbers are always positive.
    #[error("sequence number can not be zero or negative")]
    NotZeroOrNegative,

    /// Conversion to u64 from string failed.
    #[error("string contains invalid u64 value")]
    InvalidU64String,
}