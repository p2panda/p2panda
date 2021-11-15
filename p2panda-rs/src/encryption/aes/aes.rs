// SPDX-License-Identifier: AGPL-3.0-or-later

use aes_gcm::aead::{Aead, NewAead};
use aes_gcm_siv::{Aes256GcmSiv, Nonce};
use rand_core::{OsRng, RngCore};

use crate::encryption::aes::AesError;

/// Generates an unique random 96 bit nonce for AES256.
fn generate_nonce() -> Vec<u8> {
    let mut nonce_bytes = [0u8; 12]; // 12 bytes = 96 bit
    OsRng.fill_bytes(&mut nonce_bytes);
    Nonce::from_slice(&nonce_bytes).as_slice().to_vec()
}

/// Generates a new random key which can be used as the secret key for AES256.
pub fn generate_key() -> Vec<u8> {
    Aes256GcmSiv::generate_key(OsRng::default())
        .as_slice()
        .to_vec()
}

/// Encrypts plaintext data symmetrically with AES256 block cipher using a secret key, returning
/// the ciphertext and used nonce.
///
/// This method automatically generates a unique and random 96 bit nonce for every encryption to
/// avoid "accidents" where a nonce is used twice for the same key.
///
/// Panics when the key size is not 256 bit / 32 bytes.
///
/// See: https://www.imperialviolet.org/2017/05/14/aesgcmsiv.html
pub fn encrypt(key: &[u8], plaintext: &[u8]) -> Result<(Vec<u8>, Vec<u8>), AesError> {
    // Generate unique, random nonce before every encryption
    let nonce = generate_nonce();

    Aes256GcmSiv::new(key.into())
        .encrypt(nonce.as_slice().into(), plaintext)
        .map(|ciphertext| (ciphertext, nonce))
        .map_err(|_| AesError::EncryptionFailed)
}

/// Decrypt ciphertext symmetrically with AES256 block cipher using a secret key and nonce.
pub fn decrypt(key: &[u8], nonce: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, AesError> {
    Aes256GcmSiv::new(key.into())
        .decrypt(nonce.into(), ciphertext)
        .map_err(|_| AesError::DecryptionFailed)
}

#[cfg(test)]
mod test {
    use super::{decrypt, encrypt, generate_key, generate_nonce};

    #[test]
    fn unique_key_nonce() {
        assert_ne!(generate_key(), generate_key());
        assert_ne!(generate_nonce(), generate_nonce());
    }

    #[test]
    fn symmetric_encryption() {
        // Generate secret key and public nonce
        let key = generate_key();

        // Encrypt plaintext with key and retreive ciphertext and nonce
        let (ciphertext, nonce) = encrypt(&key, b"secret message").unwrap();

        // Decrypts ciphertext correctly
        let plaintext = decrypt(&key, &nonce, &ciphertext).unwrap();
        assert_eq!(&plaintext, b"secret message");

        // Throw error when invalid nonce, key or ciphertext
        assert!(decrypt(&key, &generate_nonce(), &ciphertext).is_err());
        assert!(decrypt(&generate_key(), &nonce, &ciphertext).is_err());
        assert!(decrypt(&key, &nonce, b"invalid ciphertext").is_err());
    }
}
