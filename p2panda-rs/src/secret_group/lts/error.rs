// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Custom error types for `LongTermSecret`.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum LongTermSecretError {
    /// Failed because epochs do not match.
    #[error("current epoch {0} does not match epoch {1} from ciphertext")]
    EpochNotMatching(u64, u64),

    /// Failed because group ids do not match.
    #[error("secret group id {0} does not match ciphertext {1}")]
    GroupNotMatching(String, String),

    /// Internal hashing error.
    #[error(transparent)]
    HashError(#[from] crate::hash::HashError),

    /// Internal AEAD En- & Decryption error.
    #[error(transparent)]
    CryptoError(#[from] openmls_traits::types::CryptoError),
}
