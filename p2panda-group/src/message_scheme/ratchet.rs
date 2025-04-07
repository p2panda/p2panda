// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::hash::Hash as StdHash;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto::Secret;
use crate::crypto::aead::{AeadError, AeadNonce, aead_decrypt, aead_encrypt};
use crate::crypto::hkdf::{HkdfError, hkdf};

pub const MESSAGE_KEY_SIZE: usize = 32;

pub type MessageKeyId = u64;

/// Message ratchet with support for handling lost or out-of-order messages.
///
/// Out-of-order messages cause the ratchet to "jump" ahead and keep "unused" keys persisted until
/// they're used eventually.
///
/// In this example our chain has a length of 2 in the moment an out-of-order message for key 4
/// arrives. Now we pre-generate the keys for the "jumped" messages (2 and 3), keep them persisted
/// for later. We decrypt the new message 4 with the regular now "latest" chain state.
///
/// ```text
/// 0
/// 1 <- Current chain "height"
/// 2
/// 3
/// 4 <- New chain "height" after decrypting message 4
/// ```
pub struct Ratchet;

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "test_utils"), derive(Clone))]
pub struct RatchetState {
    next_message_key: Secret<MESSAGE_KEY_SIZE>,
    next_message_id: MessageKeyId,
    ooo_message_keys: HashMap<MessageKeyId, Secret<MESSAGE_KEY_SIZE>>,
}

#[derive(Clone, Debug, PartialEq, Eq, StdHash, Serialize, Deserialize)]
pub struct RatchetCiphertext {
    pub num: u64,
    pub ciphertext: Vec<u8>,
}

impl Ratchet {
    pub fn init(bytes: [u8; MESSAGE_KEY_SIZE]) -> RatchetState {
        RatchetState {
            ooo_message_keys: HashMap::new(),
            next_message_key: Secret::from_bytes(bytes),
            next_message_id: 0,
        }
    }

    pub fn encrypt(
        y: RatchetState,
        plaintext: &[u8],
    ) -> Result<(RatchetState, RatchetCiphertext), RatchetError> {
        let num = y.next_message_id;
        let ciphertext = {
            let nonce: AeadNonce = hkdf(b"nonce", y.next_message_key.as_bytes(), None)?;
            aead_encrypt(y.next_message_key.as_bytes(), plaintext, nonce, None)?
        };
        Ok((
            Self::next_chain_key(y, false)?,
            RatchetCiphertext { num, ciphertext },
        ))
    }

    pub fn decrypt(
        y: RatchetState,
        ciphertext: &RatchetCiphertext,
    ) -> Result<(RatchetState, Vec<u8>), RatchetError> {
        let next_message_id = y.next_message_id;
        if ciphertext.num == y.next_message_id {
            // Next message uses the regular, expected ratchet key. This is the default case.
            let plaintext = Self::decrypt_inner(&y.next_message_key, &ciphertext.ciphertext)?;
            Ok((Self::next_chain_key(y, false)?, plaintext))
        } else if ciphertext.num > y.next_message_id {
            // We didn't receive some messages in-between and jumped "into the future", either
            // because they arrive out-of-order or got lost.
            let mut y_i = y;
            for _ in next_message_id..ciphertext.num {
                y_i = Self::next_chain_key(y_i, true)?;
            }
            let plaintext = Self::decrypt_inner(&y_i.next_message_key, &ciphertext.ciphertext)?;
            Ok((Self::next_chain_key(y_i, false)?, plaintext))
        } else {
            // We received a message "from the past", using a key we didn't use yet. This removes
            // the "old" key finally and does _not_ move the ratchet forward.
            let mut y_i = y;
            let Some(message_key) = y_i.ooo_message_keys.remove(&ciphertext.num) else {
                return Err(RatchetError::UnknownMessageKey(
                    ciphertext.num,
                    next_message_id,
                ));
            };
            let plaintext = Self::decrypt_inner(&message_key, &ciphertext.ciphertext)?;
            Ok((y_i, plaintext))
        }
    }

    fn decrypt_inner(
        key: &Secret<MESSAGE_KEY_SIZE>,
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, RatchetError> {
        let nonce: AeadNonce = hkdf(b"nonce", key.as_bytes(), None)?;
        let plaintext = aead_decrypt(key.as_bytes(), &ciphertext, nonce, None)?;
        Ok(plaintext)
    }

    fn next_chain_key(mut y: RatchetState, ooo: bool) -> Result<RatchetState, RatchetError> {
        if ooo {
            assert!(
                y.ooo_message_keys
                    .insert(y.next_message_id, y.next_message_key.clone())
                    .is_none(),
                "never re-use the same message id",
            );
        }

        y.next_message_key =
            Secret::from_bytes(hkdf(b"chain", y.next_message_key.as_bytes(), None)?);
        y.next_message_id += 1;

        Ok(y)
    }
}

#[derive(Debug, Error)]
pub enum RatchetError {
    #[error(transparent)]
    Hkdf(#[from] HkdfError),

    #[error(transparent)]
    Aead(#[from] AeadError),

    #[error("unknown message key {0} to decrypt (current ratchet length: {1})")]
    UnknownMessageKey(MessageKeyId, MessageKeyId),
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

    #[test]
    fn out_of_order() {
        let rng = Rng::from_seed([1; 32]);

        let update_secret: [u8; MESSAGE_KEY_SIZE] = rng.random_array().unwrap();

        let alice = Ratchet::init(update_secret);
        let bob = Ratchet::init(update_secret);

        let (alice, message_1) = Ratchet::encrypt(alice, b"Dum").unwrap();
        let (alice, message_2) = Ratchet::encrypt(alice, b"Di").unwrap();
        let (_alice, message_3) = Ratchet::encrypt(alice, b"Dum!").unwrap();

        let (bob, receive_3) = Ratchet::decrypt(bob, &message_3).unwrap();
        let (bob, receive_2) = Ratchet::decrypt(bob, &message_2).unwrap();
        let (_bob, receive_1) = Ratchet::decrypt(bob, &message_1).unwrap();

        assert_eq!(receive_1, b"Dum");
        assert_eq!(receive_2, b"Di");
        assert_eq!(receive_3, b"Dum!");
    }
}
