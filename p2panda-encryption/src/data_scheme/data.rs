// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::crypto::xchacha20::{XAeadError, XAeadNonce, x_aead_decrypt, x_aead_encrypt};
use crate::data_scheme::group_secret::GroupSecret;

pub fn encrypt_data(
    plaintext: &[u8],
    group_secret: &GroupSecret,
    nonce: XAeadNonce,
) -> Result<Vec<u8>, XAeadError> {
    let ciphertext = x_aead_encrypt(group_secret.as_bytes(), plaintext, nonce, None)?;
    Ok(ciphertext)
}

pub fn decrypt_data(
    ciphertext: &[u8],
    group_secret: &GroupSecret,
    nonce: XAeadNonce,
) -> Result<Vec<u8>, XAeadError> {
    let plaintext = x_aead_decrypt(group_secret.as_bytes(), ciphertext, nonce, None)?;
    Ok(plaintext)
}

#[cfg(test)]
mod tests {
    use crate::Rng;
    use crate::crypto::xchacha20::XAeadNonce;
    use crate::data_scheme::GroupSecret;

    use super::{decrypt_data, encrypt_data};

    #[test]
    fn encrypt_decrypt() {
        let rng = Rng::from_seed([1; 32]);

        let group_secret = GroupSecret::from_rng(&rng).unwrap();
        let nonce: XAeadNonce = rng.random_array().unwrap();

        let message = encrypt_data(b"Service! Service!", &group_secret, nonce).unwrap();
        let receive = decrypt_data(&message, &group_secret, nonce).unwrap();
        assert_eq!(receive, b"Service! Service!");
    }
}
