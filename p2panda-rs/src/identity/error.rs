// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Custom error types for key pairs.
#[derive(Error, Debug)]
pub enum KeyPairError {
    /// Handle errors from `ed25519` crate.
    #[error(transparent)]
    Ed25519(#[from] ed25519_dalek::ed25519::Error),

    /// Handle errors from `hex` crate.
    #[error(transparent)]
    HexEncoding(#[from] hex::FromHexError),
}

/// Custom error types for `Author`.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum AuthorError {
    /// Author string does not have the right length.
    #[error("invalid author key length")]
    InvalidLength,

    /// Author string contains invalid hex characters.
    #[error("invalid hex encoding in author string")]
    InvalidHexEncoding,
}
