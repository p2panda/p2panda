// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Custom error types for `Hash`.
#[derive(Error, Debug, Clone)]
#[allow(missing_copy_implementations)]
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

    /// Internal `yasmf_hash` crate error.
    #[error(transparent)]
    YasmfHashError(#[from] yasmf_hash::error::Error),
}
