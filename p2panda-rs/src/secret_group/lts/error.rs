// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Custom error types for `LongTermSecret`.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum LongTermSecretError {
    #[error("Secret epoch does not match ciphertext")]
    EpochNotMatching,

    #[error("Secret group id does not match ciphertext")]
    GroupNotMatching,

    /// AES En- & Decryption error.
    #[error(transparent)]
    AESError(#[from] crate::secret_group::aes::AesError),

    /// Hashing error.
    #[error(transparent)]
    HashError(#[from] crate::hash::HashError),
}
