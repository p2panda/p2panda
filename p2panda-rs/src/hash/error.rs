// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Custom error types for `Hash`.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum HashError {
    /// Hash string has an invalid length.
    #[error("invalid hash length")]
    InvalidLength,

    /// Hash string contains invalid hex characters.
    #[error("invalid hex encoding in hash string")]
    InvalidHexEncoding,

    /// Hash is not a valid YAMF BLAKE2b hash.
    #[error("can not decode YAMF BLAKE2b hash")]
    DecodingFailed,

    /// Internal YamfHash crate error.
    #[error(transparent)]
    YamfHashError(#[from] yamf_hash::error::Error),
}
