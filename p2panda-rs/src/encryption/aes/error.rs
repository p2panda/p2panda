// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Custom error types for AES-GCM-SIV methods.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum AesError {
    /// AES encryption failed because of an invalid key.
    #[error("AES-GCM-SIV encryption failed")]
    EncryptionFailed,

    /// AES decryption failed because the combination of key, nonce and ciphertext is not correct.
    #[error("AES-GCM-SIV decryption failed")]
    DecryptionFailed,
}
