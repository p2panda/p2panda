// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

#[derive(Error, Debug)]
pub enum HeaderBuilderError {
    #[error(transparent)]
    EncodeHeaderError(#[from] EncodeHeaderError),
}

#[derive(Error, Debug)]
pub enum EncodeHeaderError {
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
    /// Operation needs to match payload hash of encoded header.
    #[error("body needs to match payload hash of encoded header")]
    PayloadHashMismatch,

    /// Operation needs to match payload size of encoded header.
    #[error("body needs to match payload size of encoded header")]
    PayloadSizeMismatch,

    /// Could not verify authorship of operation.
    #[error("signature invalid")]
    KeyPairError(#[from] crate::identity_v2::error::KeyPairError),
}
