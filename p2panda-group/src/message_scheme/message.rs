// SPDX-License-Identifier: MIT OR Apache-2.0

use thiserror::Error;

use crate::crypto::aead::{AeadError, AeadNonce, aead_decrypt, aead_encrypt};
use crate::message_scheme::ratchet::{Generation, RatchetKeyMaterial};

pub struct Message {
    nonce: AeadNonce,
    ciphertext: Vec<u8>,
    generation: Generation,
}

pub fn encrypt_message(
    plaintext: &[u8],
    generation: Generation,
    ratchet_secrets: RatchetKeyMaterial,
) -> Result<Message, MessageError> {
    let (key, nonce) = ratchet_secrets;
    let ciphertext = aead_encrypt(key.as_bytes(), plaintext, nonce, None)?;
    Ok(Message {
        nonce,
        ciphertext,
        generation,
    })
}

pub fn decrypt_message(
    message: &Message,
    ratchet_secrets: RatchetKeyMaterial,
) -> Result<Vec<u8>, AeadError> {
    let (key, nonce) = ratchet_secrets;
    let plaintext = aead_decrypt(key.as_bytes(), &message.ciphertext, nonce, None)?;
    Ok(plaintext)
}

#[derive(Debug, Error)]
pub enum MessageError {
    #[error(transparent)]
    Aead(#[from] AeadError),
}

#[cfg(test)]
mod tests {
    use crate::Rng;
    use crate::crypto::Secret;
    use crate::crypto::aead::{AeadKey, AeadNonce};

    use super::{decrypt_message, encrypt_message};

    #[test]
    fn encrypt_decrypt() {
        let rng = Rng::from_seed([1; 32]);

        let key: AeadKey = rng.random_array().unwrap();
        let nonce: AeadNonce = rng.random_array().unwrap();

        let message_1 = encrypt_message(b"Hello!", 7, (Secret::from_bytes(key), nonce)).unwrap();
        assert_eq!(message_1.generation, 7);
        assert_eq!(message_1.nonce, nonce);

        let receive_1 = decrypt_message(&message_1, (Secret::from_bytes(key), nonce)).unwrap();
        assert_eq!(receive_1, b"Hello!");
    }
}
