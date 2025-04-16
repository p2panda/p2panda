// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::crypto::aead::{AeadError, aead_decrypt, aead_encrypt};
use crate::message_scheme::ratchet::RatchetKeyMaterial;

pub fn encrypt_message(
    plaintext: &[u8],
    ratchet_secrets: RatchetKeyMaterial,
) -> Result<Vec<u8>, AeadError> {
    let (key, nonce) = ratchet_secrets;
    let ciphertext = aead_encrypt(key.as_bytes(), plaintext, nonce, None)?;
    Ok(ciphertext)
}

pub fn decrypt_message(
    ciphertext: &[u8],
    ratchet_secrets: RatchetKeyMaterial,
) -> Result<Vec<u8>, AeadError> {
    let (key, nonce) = ratchet_secrets;
    let plaintext = aead_decrypt(key.as_bytes(), ciphertext, nonce, None)?;
    Ok(plaintext)
}

#[cfg(test)]
mod tests {
    use crate::Rng;
    use crate::crypto::Secret;
    use crate::message_scheme::{DecryptionRatchet, MESSAGE_KEY_SIZE, RatchetSecret};

    use super::{decrypt_message, encrypt_message};

    #[test]
    fn message_ratcheting() {
        let rng = Rng::from_seed([1; 32]);

        let update_secret = Secret::from_bytes(rng.random_array::<MESSAGE_KEY_SIZE>().unwrap());

        let ooo_tolerance = 4;
        let max_forward = 10;

        let alice = RatchetSecret::init(update_secret.clone());
        let bob = DecryptionRatchet::init(update_secret);

        let (alice, generation_0, alice_secret_0) = RatchetSecret::ratchet_forward(alice).unwrap();
        let message_0 = encrypt_message(b"I scream, you scream!", alice_secret_0).unwrap();

        let (bob, bob_secret_0) =
            DecryptionRatchet::secret_for_decryption(bob, generation_0, max_forward, ooo_tolerance)
                .unwrap();
        let receive_0 = decrypt_message(&message_0, bob_secret_0).unwrap();
        assert_eq!(receive_0, b"I scream, you scream!");

        let (_alice, generation_1, alice_secret_1) = RatchetSecret::ratchet_forward(alice).unwrap();
        let message_1 = encrypt_message(b"We all scream for ice-cream!", alice_secret_1).unwrap();

        let (_bob, bob_secret_1) =
            DecryptionRatchet::secret_for_decryption(bob, generation_1, max_forward, ooo_tolerance)
                .unwrap();
        let receive_1 = decrypt_message(&message_1, bob_secret_1).unwrap();
        assert_eq!(receive_1, b"We all scream for ice-cream!");
    }
}
