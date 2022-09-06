// SPDX-License-Identifier: AGPL-3.0-or-later

//! Error types for creating key pairs and validating public key representations.
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

/// Custom error types for `PublicKey`.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum PublicKeyError {
    /// Handle errors from `ed25519` crate.
    #[error(transparent)]
    Ed25519(#[from] ed25519_dalek::ed25519::Error),

    /// PublicKey string does not have the right length.
    #[error("invalid public key key length")]
    InvalidLength,

    /// PublicKey string contains invalid hex characters.
    #[error("invalid hex encoding in public key string")]
    InvalidHexEncoding,
}
