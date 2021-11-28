// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Custom error types for AES256-GCM methods.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum AesError {
    /// Failed generation of random bytes for AES nonce.
    #[error("failed generation of random bytes for aes nonce")]
    NonceGenerationFailed,

    /// AES encryption failed (invalid key).
    #[error("AES-GCM encryption failed")]
    EncryptionFailed,

    /// AES decryption failed (combination of key, nonce and ciphertext is not correct).
    #[error("AES-GCM decryption failed")]
    DecryptionFailed,
}
