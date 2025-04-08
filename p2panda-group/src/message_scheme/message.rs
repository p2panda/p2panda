// SPDX-License-Identifier: MIT OR Apache-2.0

use thiserror::Error;

use crate::crypto::Secret;
use crate::crypto::aead::{AeadError, AeadNonce, aead_decrypt, aead_encrypt};
use crate::crypto::hkdf::{HkdfError, hkdf};
use crate::message_scheme::ratchet::MESSAGE_KEY_SIZE;

pub fn encrypt_message(
    plaintext: &[u8],
    ratchet_secret: Secret<MESSAGE_KEY_SIZE>,
) -> Result<Vec<u8>, MessageError> {
    let ciphertext = {
        let nonce: AeadNonce = hkdf(b"nonce", ratchet_secret.as_bytes(), None)?;
        aead_encrypt(ratchet_secret.as_bytes(), plaintext, nonce, None)?
    };
    Ok(ciphertext)
}

pub fn decrypt_message(
    ciphertext: &[u8],
    ratchet_secret: Secret<MESSAGE_KEY_SIZE>,
) -> Result<Vec<u8>, MessageError> {
    let plaintext = {
        let nonce: AeadNonce = hkdf(b"nonce", ratchet_secret.as_bytes(), None)?;
        aead_decrypt(ratchet_secret.as_bytes(), ciphertext, nonce, None)?
    };
    Ok(plaintext)
}

#[derive(Debug, Error)]
pub enum MessageError {
    #[error(transparent)]
    Hkdf(#[from] HkdfError),

    #[error(transparent)]
    Aead(#[from] AeadError),
}

#[cfg(test)]
mod tests {
    use crate::Rng;
    use crate::crypto::Secret;
    use crate::message_scheme::MESSAGE_KEY_SIZE;

    use super::{decrypt_message, encrypt_message};

    #[test]
    fn encrypt_decrypt() {
        let rng = Rng::from_seed([1; 32]);

        let key: [u8; MESSAGE_KEY_SIZE] = rng.random_array().unwrap();

        let message_1 = encrypt_message(b"Hello!", Secret::from_bytes(key)).unwrap();
        let receive_1 = decrypt_message(&message_1, Secret::from_bytes(key)).unwrap();
        assert_eq!(receive_1, b"Hello!");
    }
}
