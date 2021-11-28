// SPDX-License-Identifier: AGPL-3.0-or-later

use aes_gcm::aead::{Aead, NewAead};
use aes_gcm::Aes256Gcm;

use crate::secret_group::aes::AesError;

/// Encrypts plaintext data symmetrically with AES256 block cipher using a secret key and nonce,
/// returning the ciphertext.
pub fn encrypt(key: &[u8], nonce: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, AesError> {
    Aes256Gcm::new(key.into())
        .encrypt(nonce.into(), plaintext)
        .map_err(|_| AesError::EncryptionFailed)
}

/// Decrypt ciphertext symmetrically with AES256 block cipher using a secret key and nonce.
pub fn decrypt(key: &[u8], nonce: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, AesError> {
    Aes256Gcm::new(key.into())
        .decrypt(nonce.into(), ciphertext)
        .map_err(|_| AesError::DecryptionFailed)
}

#[cfg(test)]
mod test {
    use openmls_traits::{random::OpenMlsRand, OpenMlsCryptoProvider};

    use crate::secret_group::mls::MlsProvider;

    use super::{decrypt, encrypt};

    // Helper method to generate a new random key which can be used as the secret key for AES256
    fn generate_key(provider: &impl OpenMlsCryptoProvider) -> Vec<u8> {
        provider.rand().random_vec(32).unwrap()
    }

    fn generate_nonce(provider: &impl OpenMlsCryptoProvider) -> Vec<u8> {
        provider.rand().random_vec(12).unwrap()
    }

    #[test]
    fn encrypt_decrypt() {
        let provider = MlsProvider::new();

        // Generate secret key and public nonce
        let key = generate_key(&provider);
        let nonce = generate_nonce(&provider);

        // Encrypt plaintext with key and retreive ciphertext and nonce
        let ciphertext = encrypt(&key, &nonce, b"secret message").unwrap();

        // Decrypts ciphertext correctly
        let plaintext = decrypt(&key, &nonce, &ciphertext).unwrap();
        assert_eq!(&plaintext, b"secret message");

        // Throw error when invalid nonce, key or ciphertext
        assert!(decrypt(&key, &generate_nonce(&provider), &ciphertext).is_err());
        assert!(decrypt(&generate_key(&provider), &nonce, &ciphertext).is_err());
        assert!(decrypt(&key, &nonce, b"invalid ciphertext").is_err());
    }
}
