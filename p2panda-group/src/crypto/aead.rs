// SPDX-License-Identifier: MIT OR Apache-2.0

//! ChaCha20Poly1305 authenticated encryption with additional data (AEAD).
use libcrux_chacha20poly1305::{KEY_LEN, NONCE_LEN, TAG_LEN};
use thiserror::Error;

/// 96-bit nonce.
pub type AeadNonce = [u8; NONCE_LEN];

/// 256-bit secret key.
pub type AeadKey = [u8; KEY_LEN];

/// ChaCha20Poly1305 AEAD encryption function.
pub fn aead_encrypt(
    key: &AeadKey,
    plaintext: &[u8],
    nonce: AeadNonce,
    aad: Option<&[u8]>,
) -> Result<Vec<u8>, AeadError> {
    // Implementation attaches authenticated tag (16 bytes) automatically to the end of ciphertext.
    let mut ciphertext_tag = vec![0; plaintext.len() + TAG_LEN];
    libcrux_chacha20poly1305::encrypt(
        key,
        plaintext,
        &mut ciphertext_tag,
        aad.unwrap_or_default(),
        &nonce,
    )
    .map_err(AeadError::Encrypt)?;
    Ok(ciphertext_tag)
}

/// ChaCha20Poly1305 AEAD decryption function.
pub fn aead_decrypt(
    key: &AeadKey,
    ciphertext_tag: &[u8],
    nonce: AeadNonce,
    aad: Option<&[u8]>,
) -> Result<Vec<u8>, AeadError> {
    let mut buffer = vec![0; ciphertext_tag.len()];
    let plaintext = libcrux_chacha20poly1305::decrypt(
        key,
        &mut buffer,
        ciphertext_tag,
        aad.unwrap_or_default(),
        &nonce,
    )
    .map_err(AeadError::Decrypt)?;
    Ok(plaintext.to_vec())
}

#[derive(Debug, Error)]
pub enum AeadError {
    #[error("plaintext could not be encrypted with aead: {0}")]
    Encrypt(libcrux_chacha20poly1305::AeadError),

    #[error("ciphertext could not be decrypted with aead: {0}")]
    Decrypt(libcrux_chacha20poly1305::AeadError),
}

#[cfg(test)]
mod tests {
    use crate::crypto::Crypto;
    use crate::traits::RandProvider;

    use super::{AeadError, AeadKey, AeadNonce, aead_decrypt, aead_encrypt};

    #[test]
    fn encrypt_decrypt() {
        let rng = Crypto::from_seed([1; 32]);

        let key: AeadKey = rng.random_array().unwrap();
        let nonce: AeadNonce = rng.random_array().unwrap();

        let ciphertext = aead_encrypt(&key, b"Hello, Panda!", nonce, None).unwrap();
        let plaintext = aead_decrypt(&key, &ciphertext, nonce, None).unwrap();

        assert_eq!(plaintext, b"Hello, Panda!");
    }

    #[test]
    fn decryption_failed() {
        let rng = Crypto::from_seed([1; 32]);

        let key: AeadKey = rng.random_array().unwrap();
        let nonce: AeadNonce = rng.random_array().unwrap();

        let ciphertext = aead_encrypt(&key, b"Hello, Panda!", nonce, None).unwrap();

        let invalid_key: AeadKey = rng.random_array().unwrap();
        let invalid_nonce: AeadNonce = rng.random_array().unwrap();

        // Invalid key.
        assert!(matches!(
            aead_decrypt(&invalid_key, &ciphertext, nonce, None),
            Err(AeadError::Decrypt(
                libcrux_chacha20poly1305::AeadError::InvalidCiphertext
            ))
        ));

        // Invalid nonce.
        assert!(matches!(
            aead_decrypt(&key, &ciphertext, invalid_nonce, None),
            Err(AeadError::Decrypt(
                libcrux_chacha20poly1305::AeadError::InvalidCiphertext
            ))
        ));

        // Invalid additional data.
        assert!(matches!(
            aead_decrypt(&key, &ciphertext, nonce, Some(b"invalid aad")),
            Err(AeadError::Decrypt(
                libcrux_chacha20poly1305::AeadError::InvalidCiphertext
            ))
        ));
    }
}
