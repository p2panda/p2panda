// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

#[derive(Error, Debug)]
pub enum DocumentLinksError {
    /// Document id was set but not previous.
    #[error("if document id is set then previous is also expected")]
    ExpectedPrevious,

    /// previous was set but not document id.
    #[error("if previous is set then document id is also expected")]
    ExpectedDocumentId,
}

#[derive(Error, Debug)]
pub enum HeaderBuilderError {
    #[error(transparent)]
    EncodeHeaderError(#[from] EncodeHeaderError),

    #[error(transparent)]
    ValidateHeaderError(#[from] ValidateHeaderError),

    #[error(transparent)]
    DocumentLinksError(#[from] DocumentLinksError),
}

#[derive(Error, Debug)]
pub enum EncodeHeaderError {
    /// CBOR encoder failed critically due to an IO issue.
    #[error("cbor encoder failed {0}")]
    EncoderIOFailed(String),

    /// CBOR encoder could not serialize this value.
    #[error("cbor encoder failed serializing value {0}")]
    EncoderFailed(String),
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

    /// Header assumed to be CREATE cannot have a non-zero sequence number.
    #[error("header assumed to be CREATE cannot have a non-zero sequence number")]
    CreateUnexpectedNonZeroSeqNum,

    /// Header assumed to be UPDATE must contain document id and previous.
    #[error("header assumed to be UPDATE must contain document id and previous")]
    UpdateExpectedDocumentIdAndPrevious,

    /// DELETE header must contain document id and previous.
    #[error("DELETE header must contain document id and previous")]
    DeleteExpectedDocumentIdAndPrevious,

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
