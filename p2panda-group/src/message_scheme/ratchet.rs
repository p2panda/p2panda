// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto::Secret;
use crate::crypto::aead::{AeadKey, AeadNonce};
use crate::crypto::hkdf::{HkdfError, hkdf};

pub const MESSAGE_KEY_SIZE: usize = 32;

/// Key generation of message ratchet.
pub type Generation = u32;

pub type RatchetKeyMaterial = (Secret<MESSAGE_KEY_SIZE>, AeadNonce);

#[derive(Debug)]
pub struct RatchetSecret;

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "test_utils"), derive(Clone))]
pub struct RatchetSecretState {
    secret: Secret<MESSAGE_KEY_SIZE>,
    generation: Generation,
}

impl RatchetSecret {
    pub fn init(secret: Secret<MESSAGE_KEY_SIZE>) -> RatchetSecretState {
        RatchetSecretState {
            secret,
            generation: 0,
        }
    }

    pub fn ratchet_forward(
        mut y: RatchetSecretState,
    ) -> Result<(RatchetSecretState, Generation, RatchetKeyMaterial), RatchetError> {
        let generation = y.generation;

        let key: AeadKey = hkdf(b"key", y.secret.as_bytes(), None)?;
        let nonce: AeadNonce = hkdf(b"nonce", y.secret.as_bytes(), None)?;

        y.generation += 1;
        y.secret = Secret::from_bytes(hkdf(b"chain", y.secret.as_bytes(), None)?);

        Ok((y, generation, (Secret::from_bytes(key), nonce)))
    }
}

#[derive(Debug)]
pub struct DecryptionRatchet;

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "test_utils"), derive(Clone))]
pub struct DecryptionRatchetState {
    past_secrets: VecDeque<Option<RatchetKeyMaterial>>,
    ratchet_head: RatchetSecretState,
}

impl DecryptionRatchet {
    pub fn init(secret: Secret<MESSAGE_KEY_SIZE>) -> DecryptionRatchetState {
        DecryptionRatchetState {
            past_secrets: VecDeque::new(),
            ratchet_head: RatchetSecret::init(secret),
        }
    }

    fn prune_past_secrets(
        mut y: DecryptionRatchetState,
        ooo_tolerance: u32,
    ) -> DecryptionRatchetState {
        y.past_secrets.truncate(ooo_tolerance as usize);
        y
    }

    pub fn secret_for_decryption(
        mut y: DecryptionRatchetState,
        generation: Generation,
        maximum_forward_distance: u32,
        ooo_tolerance: u32,
    ) -> Result<(DecryptionRatchetState, RatchetKeyMaterial), RatchetError> {
        let current_generation = y.ratchet_head.generation;

        // If generation is too distant in the future
        if current_generation < u32::MAX - maximum_forward_distance
            && generation > current_generation + maximum_forward_distance
        {
            return Err(RatchetError::TooDistantInTheFuture);
        }

        // If generation id too distant in the past
        if generation < current_generation && (current_generation - generation) > ooo_tolerance {
            return Err(RatchetError::TooDistantInThePast);
        }

        // If generation is the one the ratchet is currently at or in the future.
        if generation >= current_generation {
            // Ratchet the chain forward as far as necessary
            for _ in 0..(generation - current_generation) {
                // Derive the key material
                let (y_ratchet_head_i, _, ratchet_secrets) =
                    RatchetSecret::ratchet_forward(y.ratchet_head)?;
                y.ratchet_head = y_ratchet_head_i;
                // Add it to the front of the queue
                y.past_secrets.push_front(Some(ratchet_secrets));
            }
            let (y_ratchet_head_i, _, ratchet_secrets) =
                RatchetSecret::ratchet_forward(y.ratchet_head)?;
            y.ratchet_head = y_ratchet_head_i;
            // Add an entry to the past secrets queue to keep indexing consistent.
            y.past_secrets.push_front(None);
            let y_i = Self::prune_past_secrets(y, ooo_tolerance);
            Ok((y_i, ratchet_secrets))
        } else {
            // If the requested generation is within the window of past secrets, we should get a
            // positive index.
            let window_index = ((current_generation - generation) as i32) - 1;
            // We might not have the key material (e.g. we might have discarded it when generating
            // an encryption secret).
            let index = if window_index >= 0 {
                window_index as usize
            } else {
                return Err(RatchetError::TooDistantInThePast);
            };
            // Get the relevant secrets from the past secrets queue.
            let ratchet_secrets = y
                .past_secrets
                .get_mut(index)
                .ok_or(RatchetError::IndexOutOfBounds)?
                // We use take here to replace the entry in the `past_secrets` with `None`, thus
                // achieving FS for that secret as soon as the caller of this function drops it.
                .take()
                // If the requested generation was used to decrypt a message earlier, throw an
                // error.
                .ok_or(RatchetError::SecretReuseError)?;
            Ok((y, ratchet_secrets))
        }
    }
}

#[derive(Debug, Error)]
pub enum RatchetError {
    #[error(transparent)]
    Hkdf(#[from] HkdfError),

    #[error("")]
    TooDistantInTheFuture,

    #[error("")]
    TooDistantInThePast,

    #[error("")]
    IndexOutOfBounds,

    #[error("")]
    SecretReuseError,
}
