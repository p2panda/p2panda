// SPDX-License-Identifier: MIT OR Apache-2.0

//! AES-256-GCM authenticated symmetric encryption with additional data (AEAD) with 256-bit key,
//! 16-bit tag and 96-bit nonce.
use libcrux::aead::{Algorithm, Iv, Key, Tag, decrypt_detached, encrypt_detached};
use thiserror::Error;

const AEAD_ALGORITHM: Algorithm = Algorithm::Aes256Gcm;

pub type AeadNonce = [u8; AEAD_ALGORITHM.nonce_size()];

pub type AeadKey = [u8; AEAD_ALGORITHM.key_size()];

pub fn aead_encrypt(
    key: &AeadKey,
    plaintext: &[u8],
    nonce: AeadNonce,
    aad: Option<&[u8]>,
) -> Result<Vec<u8>, AeadError> {
    let key = Key::from_slice(AEAD_ALGORITHM, key).map_err(AeadError::InvalidArgument)?;
    let nonce = Iv::new(nonce).map_err(AeadError::InvalidArgument)?;

    let (tag, mut ciphertext) = encrypt_detached(&key, plaintext, nonce, aad.unwrap_or_default())
        .map_err(AeadError::Encrypt)?;

    // Attach authenticated tag to the end of ciphertext.
    ciphertext.extend_from_slice(tag.as_ref());

    Ok(ciphertext)
}

pub fn aead_decrypt(
    key: &AeadKey,
    ciphertext_tag: &[u8],
    nonce: AeadNonce,
    aad: Option<&[u8]>,
) -> Result<Vec<u8>, AeadError> {
    if ciphertext_tag.len() < AEAD_ALGORITHM.tag_size() {
        return Err(AeadError::InvalidArgument(
            libcrux::aead::InvalidArgumentError::InvalidTag,
        ));
    }

    // Extract authenticated tag from the end of ciphertext.
    let boundary = ciphertext_tag.len() - AEAD_ALGORITHM.tag_size();
    let ciphertext = &ciphertext_tag[..boundary];
    let tag = &ciphertext_tag[boundary..];

    let key = Key::from_slice(AEAD_ALGORITHM, key).map_err(AeadError::InvalidArgument)?;
    let nonce = Iv::new(nonce).map_err(AeadError::InvalidArgument)?;
    let tag = Tag::from_slice(tag).map_err(AeadError::InvalidArgument)?;

    let plaintext = decrypt_detached(&key, ciphertext, nonce, aad.unwrap_or_default(), &tag)
        .map_err(AeadError::Decrypt)?;

    Ok(plaintext)
}

#[derive(Debug, Error)]
pub enum AeadError {
    #[error("invalid aead argument: {0}")]
    InvalidArgument(libcrux::aead::InvalidArgumentError),

    #[error("could not encrypt with aead: {0}")]
    Encrypt(libcrux::aead::EncryptError),

    #[error("could not decrypt with aead: {0}")]
    Decrypt(libcrux::aead::DecryptError),
}

#[cfg(test)]
mod tests {
    use crate::crypto::provider::Provider;
    use crate::crypto::traits::RandProvider;

    use super::{AeadError, AeadKey, AeadNonce, aead_decrypt, aead_encrypt};

    #[test]
    fn encrypt_decrypt() {
        let rng = Provider::from_seed([1; 32]);

        let key: AeadKey = rng.random_array().unwrap();
        let nonce: AeadNonce = rng.random_array().unwrap();

        let ciphertext = aead_encrypt(&key, b"Hello, Panda!", nonce, None).unwrap();
        let plaintext = aead_decrypt(&key, &ciphertext, nonce, None).unwrap();

        assert_eq!(plaintext, b"Hello, Panda!");
    }

    #[test]
    fn decryption_failed() {
        let rng = Provider::from_seed([1; 32]);

        let key: AeadKey = rng.random_array().unwrap();
        let nonce: AeadNonce = rng.random_array().unwrap();

        let ciphertext = aead_encrypt(&key, b"Hello, Panda!", nonce, None).unwrap();

        let invalid_key: AeadKey = rng.random_array().unwrap();
        let result = aead_decrypt(&invalid_key, &ciphertext, nonce, None);

        assert!(matches!(
            result,
            Err(AeadError::Decrypt(
                libcrux::aead::DecryptError::DecryptionFailed
            ))
        ));
    }
}
