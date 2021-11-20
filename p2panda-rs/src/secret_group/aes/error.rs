// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Custom error types for AES256-GCM-SIV methods.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum AesError {
    /// Failed generation of random bytes for AES nonce.
    #[error("Failed generation of random bytes for AES nonce")]
    NonceGenerationFailed,

    /// AES encryption failed (invalid key).
    #[error("AES-GCM-SIV encryption failed")]
    EncryptionFailed,

    /// AES decryption failed (combination of key, nonce and ciphertext is not correct).
    #[error("AES-GCM-SIV decryption failed")]
    DecryptionFailed,
}
