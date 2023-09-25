// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

#[derive(Error, Debug)]
pub enum EncodeBodyError {
    /// CBOR encoder failed critically due to an IO issue.
    #[error("cbor encoder failed {0}")]
    EncoderIOFailed(String),

    /// CBOR encoder could not serialize this value.
    #[error("cbor encoder failed serializing value {0}")]
    EncoderFailed(String),
}

#[derive(Error, Debug)]
pub enum DecodeBodyError {
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

/// Errors from `PlainValue` enum.
#[derive(Error, Debug)]
pub enum PlainValueError {
    /// Error resulting from failure to parsing a byte string into a String.
    #[error("attempted to parse non-utf8 bytes into string")]
    BytesNotUtf8,

    /// Handle errors when converting from an integer.
    #[error(transparent)]
    IntError(#[from] std::num::TryFromIntError),

    /// Tried to parse a PlainValue from an unsupported cbor value.
    #[error("data did not match any variant of untagged enum PlainValue")]
    UnsupportedValue,
}
