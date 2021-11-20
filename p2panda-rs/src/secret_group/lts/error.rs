// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Custom error types for `LongTermSecret`.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum LongTermSecretError {
    /// Failed because epochs do not match.
    #[error("Secret epoch does not match ciphertext")]
    EpochNotMatching,

    /// Failed because group ids do not match.
    #[error("Secret group id does not match ciphertext")]
    GroupNotMatching,

    /// Internal AES En- & Decryption error.
    #[error(transparent)]
    AESError(#[from] crate::secret_group::aes::AesError),

    /// Internal hashing error.
    #[error(transparent)]
    HashError(#[from] crate::hash::HashError),
}
