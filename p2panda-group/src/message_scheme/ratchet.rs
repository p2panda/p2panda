// SPDX-License-Identifier: MIT OR Apache-2.0

use std::hash::Hash as StdHash;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto::Secret;
use crate::crypto::aead::{AeadError, AeadNonce, aead_decrypt, aead_encrypt};
use crate::crypto::hkdf::{HkdfError, hkdf};

pub const MESSAGE_KEY_SIZE: usize = 32;

/// Message ratchet with handling of lost or out-of-order messages.
pub struct Ratchet;

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "test_utils"), derive(Clone))]
pub struct RatchetState {
    next_message_key: Secret<MESSAGE_KEY_SIZE>,
    next_message_num: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, StdHash, Serialize, Deserialize)]
pub struct RatchetCiphertext {
    pub num: u64,
    pub ciphertext: Vec<u8>,
}

impl Ratchet {
    pub fn init(bytes: [u8; MESSAGE_KEY_SIZE]) -> RatchetState {
        RatchetState {
            next_message_key: Secret::from_bytes(bytes),
            next_message_num: 0,
        }
    }

    pub fn encrypt(
        y: RatchetState,
        plaintext: &[u8],
    ) -> Result<(RatchetState, RatchetCiphertext), RatchetError> {
        let num = y.next_message_num;
        let ciphertext = {
            let nonce: AeadNonce = hkdf(b"nonce", y.next_message_key.as_bytes(), None)?;
            aead_encrypt(y.next_message_key.as_bytes(), plaintext, nonce, None)?
        };
        Ok((
            Self::next_chain_key(y)?,
            RatchetCiphertext { num, ciphertext },
        ))
    }

    pub fn decrypt(
        y: RatchetState,
        ciphertext: &RatchetCiphertext,
    ) -> Result<(RatchetState, Vec<u8>), RatchetError> {
        let plaintext = {
            let nonce: AeadNonce = hkdf(b"nonce", y.next_message_key.as_bytes(), None)?;
            aead_decrypt(
                y.next_message_key.as_bytes(),
                &ciphertext.ciphertext,
                nonce,
                None,
            )?
        };
        Ok((Self::next_chain_key(y)?, plaintext))
    }

    fn next_chain_key(mut y: RatchetState) -> Result<RatchetState, RatchetError> {
        y.next_message_key =
            Secret::from_bytes(hkdf(b"chain", y.next_message_key.as_bytes(), None)?);
        y.next_message_num += 1;
        Ok(y)
    }
}

#[derive(Debug, Error)]
pub enum RatchetError {
    #[error(transparent)]
    Hkdf(#[from] HkdfError),

    #[error(transparent)]
    Aead(#[from] AeadError),
}

#[cfg(test)]
mod tests {
    use crate::Rng;

    use super::{MESSAGE_KEY_SIZE, Ratchet};

    #[test]
    fn encrypt_decrypt() {
        let rng = Rng::from_seed([1; 32]);

        let update_secret: [u8; MESSAGE_KEY_SIZE] = rng.random_array().unwrap();

        let alice = Ratchet::init(update_secret);
        let bob = Ratchet::init(update_secret);

        let (alice, message_1) = Ratchet::encrypt(alice, b"Dum").unwrap();
        let (bob, receive_1) = Ratchet::decrypt(bob, &message_1).unwrap();
        assert_eq!(message_1.num, 0);

        let (alice, message_2) = Ratchet::encrypt(alice, b"Di").unwrap();
        let (bob, receive_2) = Ratchet::decrypt(bob, &message_2).unwrap();
        assert_eq!(message_2.num, 1);

        let (_alice, message_3) = Ratchet::encrypt(alice, b"Dum!").unwrap();
        let (_bob, receive_3) = Ratchet::decrypt(bob, &message_3).unwrap();
        assert_eq!(message_3.num, 2);

        assert_eq!(receive_1, b"Dum");
        assert_eq!(receive_2, b"Di");
        assert_eq!(receive_3, b"Dum!");
    }
}
