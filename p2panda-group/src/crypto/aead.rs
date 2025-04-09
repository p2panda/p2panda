// SPDX-License-Identifier: MIT OR Apache-2.0

//! ChaCha20Poly1305 authenticated encryption with additional data (AEAD).
//!
//! <https://www.rfc-editor.org/rfc/rfc7905>
use chacha20poly1305::{AeadInPlace, ChaCha20Poly1305, Key, KeyInit, Nonce};
use thiserror::Error;

/// 96-bit nonce.
pub type AeadNonce = [u8; 12];

/// 256-bit secret key.
pub type AeadKey = [u8; 32];

/// ChaCha20Poly1305 AEAD encryption function.
pub fn aead_encrypt(
    key: &AeadKey,
    plaintext: &[u8],
    nonce: AeadNonce,
    aad: Option<&[u8]>,
) -> Result<Vec<u8>, AeadError> {
    // Implementation attaches authenticated tag (16 bytes) automatically to the end of ciphertext.
    let key = Key::from_slice(key);
    let nonce = Nonce::from_slice(&nonce);
    let mut ciphertext_with_tag: Vec<u8> = Vec::from(plaintext);
    let cipher = ChaCha20Poly1305::new(key);
    cipher
        .encrypt_in_place(nonce, aad.unwrap_or_default(), &mut ciphertext_with_tag)
        .map_err(AeadError::Encrypt)?;
    Ok(ciphertext_with_tag)
}

/// ChaCha20Poly1305 AEAD decryption function.
pub fn aead_decrypt(
    key: &AeadKey,
    ciphertext_with_tag: &[u8],
    nonce: AeadNonce,
    aad: Option<&[u8]>,
) -> Result<Vec<u8>, AeadError> {
    let key = Key::from_slice(key);
    let nonce = Nonce::from_slice(&nonce);
    let mut plaintext: Vec<u8> = Vec::from(ciphertext_with_tag);
    let cipher = ChaCha20Poly1305::new(key);
    cipher
        .decrypt_in_place(nonce, aad.unwrap_or_default(), &mut plaintext)
        .map_err(AeadError::Decrypt)?;
    Ok(plaintext)
}

#[derive(Debug, Error)]
pub enum AeadError {
    #[error("plaintext could not be encrypted with aead: {0}")]
    Encrypt(chacha20poly1305::Error),

    #[error("ciphertext could not be decrypted with aead: {0}")]
    Decrypt(chacha20poly1305::Error),
}

#[cfg(test)]
mod tests {
    use crate::crypto::Rng;

    use super::{AeadError, AeadKey, AeadNonce, aead_decrypt, aead_encrypt};

    #[test]
    fn encrypt_decrypt() {
        let rng = Rng::from_seed([1; 32]);

        let key: AeadKey = rng.random_array().unwrap();
        let nonce: AeadNonce = rng.random_array().unwrap();

        let ciphertext = aead_encrypt(&key, b"Hello, Panda!", nonce, None).unwrap();
        let plaintext = aead_decrypt(&key, &ciphertext, nonce, None).unwrap();

        assert_eq!(plaintext, b"Hello, Panda!");
    }

    #[test]
    fn decryption_failed() {
        let rng = Rng::from_seed([1; 32]);

        let key: AeadKey = rng.random_array().unwrap();
        let nonce: AeadNonce = rng.random_array().unwrap();

        let ciphertext = aead_encrypt(&key, b"Hello, Panda!", nonce, None).unwrap();

        let invalid_key: AeadKey = rng.random_array().unwrap();
        let invalid_nonce: AeadNonce = rng.random_array().unwrap();

        // Invalid key.
        assert!(matches!(
            aead_decrypt(&invalid_key, &ciphertext, nonce, None),
            Err(AeadError::Decrypt(chacha20poly1305::Error))
        ));

        // Invalid nonce.
        assert!(matches!(
            aead_decrypt(&key, &ciphertext, invalid_nonce, None),
            Err(AeadError::Decrypt(chacha20poly1305::Error))
        ));

        // Invalid additional data.
        assert!(matches!(
            aead_decrypt(&key, &ciphertext, nonce, Some(b"invalid aad")),
            Err(AeadError::Decrypt(chacha20poly1305::Error))
        ));
    }
}
