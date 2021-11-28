// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Custom error types for `LongTermSecret`.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum LongTermSecretError {
    /// Failed because epochs do not match.
    #[error("secret epoch does not match ciphertext")]
    EpochNotMatching,

    /// Failed because group ids do not match.
    #[error("secret group id does not match ciphertext")]
    GroupNotMatching,

    /// Internal hashing error.
    #[error(transparent)]
    HashError(#[from] crate::hash::HashError),

    /// Internal AEAD En- & Decryption error.
    #[error(transparent)]
    CryptoError(#[from] openmls_traits::types::CryptoError),
}
