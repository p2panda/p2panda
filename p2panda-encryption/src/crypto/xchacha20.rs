// SPDX-License-Identifier: MIT OR Apache-2.0

//! XChaCha20Poly1305 is a ChaCha20 AEAD variant with an extended 192-bit (24-byte) nonce.
use chacha20poly1305::{AeadInPlace, Key, KeyInit, XChaCha20Poly1305, XNonce};
use thiserror::Error;

/// "Extended" 192-bit nonce.
pub type XAeadNonce = [u8; 24];

/// 256-bit key.
pub type XAeadKey = [u8; 32];

/// XChaCha20-Poly1305 AEAD encryption function.
pub fn x_aead_encrypt(
    key: &XAeadKey,
    plaintext: &[u8],
    nonce: XAeadNonce,
    aad: Option<&[u8]>,
) -> Result<Vec<u8>, XAeadError> {
    let key = Key::from_slice(key);
    let nonce = XNonce::from_slice(&nonce);
    let mut ciphertext: Vec<u8> = Vec::from(plaintext);

    let cipher = XChaCha20Poly1305::new(key);
    cipher
        .encrypt_in_place(nonce, aad.unwrap_or_default(), &mut ciphertext)
        .map_err(XAeadError::Encrypt)?;

    Ok(ciphertext)
}

/// XChaCha20-Poly1305 AEAD decryption function.
pub fn x_aead_decrypt(
    key: &XAeadKey,
    ciphertext_with_tag: &[u8],
    nonce: XAeadNonce,
    aad: Option<&[u8]>,
) -> Result<Vec<u8>, XAeadError> {
    let key = Key::from_slice(key);
    let nonce = XNonce::from_slice(&nonce);
    let mut plaintext: Vec<u8> = Vec::from(ciphertext_with_tag);

    let cipher = XChaCha20Poly1305::new(key);
    cipher
        .decrypt_in_place(nonce, aad.unwrap_or_default(), &mut plaintext)
        .map_err(XAeadError::Decrypt)?;

    Ok(plaintext)
}

#[derive(Debug, Error)]
pub enum XAeadError {
    #[error("plaintext could not be encrypted with xchacha20 aead: {0}")]
    Encrypt(chacha20poly1305::Error),

    #[error("ciphertext could not be decrypted with xchacha20 aead: {0}")]
    Decrypt(chacha20poly1305::Error),
}

#[cfg(test)]
mod tests {
    use crate::crypto::Rng;

    use super::{XAeadError, XAeadKey, XAeadNonce, x_aead_decrypt, x_aead_encrypt};

    #[test]
    fn encrypt_decrypt() {
        let rng = Rng::from_seed([1; 32]);

        let key: XAeadKey = rng.random_array().unwrap();
        let nonce: XAeadNonce = rng.random_array().unwrap();

        let ciphertext = x_aead_encrypt(&key, b"Hello, Panda!", nonce, None).unwrap();
        let plaintext = x_aead_decrypt(&key, &ciphertext, nonce, None).unwrap();

        assert_eq!(plaintext, b"Hello, Panda!");
    }

    #[test]
    fn decryption_failed() {
        let rng = Rng::from_seed([1; 32]);

        let key: XAeadKey = rng.random_array().unwrap();
        let nonce: XAeadNonce = rng.random_array().unwrap();

        let ciphertext = x_aead_encrypt(&key, b"Hello, Panda!", nonce, None).unwrap();

        let invalid_key: XAeadKey = rng.random_array().unwrap();
        let invalid_nonce: XAeadNonce = rng.random_array().unwrap();

        // Invalid key.
        assert!(matches!(
            x_aead_decrypt(&invalid_key, &ciphertext, nonce, None),
            Err(XAeadError::Decrypt(chacha20poly1305::Error))
        ));

        // Invalid nonce.
        assert!(matches!(
            x_aead_decrypt(&key, &ciphertext, invalid_nonce, None),
            Err(XAeadError::Decrypt(chacha20poly1305::Error))
        ));

        // Invalid additional data.
        assert!(matches!(
            x_aead_decrypt(&key, &ciphertext, nonce, Some(b"invalid aad")),
            Err(XAeadError::Decrypt(chacha20poly1305::Error))
        ));
    }
}
