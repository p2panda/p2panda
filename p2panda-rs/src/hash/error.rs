// SPDX-License-Identifier: AGPL-3.0-or-later

//! Error types for validating or creating a hash.
use thiserror::Error;

/// Error types for `Hash` struct.
#[derive(Error, Debug)]
pub enum HashError {
    /// Hash string has an invalid length.
    #[error("invalid hash length {0} bytes, expected {1} bytes")]
    InvalidLength(usize, usize),

    /// Hash string contains invalid hex characters.
    #[error("invalid hex encoding in hash string")]
    InvalidHexEncoding,

    /// Hash is not a valid YASMF BLAKE3 hash.
    #[error("can not decode YASMF BLAKE3 hash")]
    DecodingFailed,

    /// Internal error from `yasmf_hash` crate.
    #[error(transparent)]
    YasmfHashError(#[from] yasmf_hash::error::Error),
}
