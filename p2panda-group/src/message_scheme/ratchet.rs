// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto::Secret;
use crate::crypto::hkdf::{HkdfError, hkdf};

pub const MESSAGE_KEY_SIZE: usize = 32;

/// Key generation of message ratchet.
pub type Generation = u64;

/// Message ratchet that can output key material either for encryption or decryption.
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

    /// Consume the current ratchet secret to derive key material and establish a new secret for
    /// the next generation.
    pub fn ratchet_forward(
        mut y: RatchetSecretState,
    ) -> Result<(RatchetSecretState, Generation, Secret<MESSAGE_KEY_SIZE>), RatchetError> {
        // Derive key material from current secret.
        let generation = y.generation;
        let secret: [u8; MESSAGE_KEY_SIZE] = hkdf(b"key", y.secret.as_bytes(), None)?;

        // Ratchet forward.
        y.generation += 1;
        y.secret = Secret::from_bytes(hkdf(b"chain", y.secret.as_bytes(), None)?);

        Ok((y, generation, Secret::from_bytes(secret)))
    }
}

/// Message ratchet for decryption with support for handling lost or out-of-order messages.
///
/// ## Out-of-order handling
///
/// Out-of-order messages cause the ratchet to "jump" ahead and keep "unused" keys persisted until
/// they're used eventually.
///
/// In this example our chain has a length of 2 in the moment an message for generation 4 arrives
/// out of order (we've expected generation 2). Now we pre-generate the keys for the "jumped"
/// messages (generation 2 and 3), keep them persisted for later. We decrypt the new message for
/// generation 4 with the regular, now "latest", chain state.
///
/// ```text
/// 0
/// 1 <- Current chain "head"
/// 2
/// 3
/// 4 <- New chain "head" after receiving message @ generation 4
/// ```
///
/// ## Tolerance limits
///
/// Developers can and should set bounds to how much a decryption ratchet can tolerate messages
/// arriving out of order, that is, into the "future" and into the "past". Setting these "window"
/// limits has implications for the forward-secrecy of an application as unused keys stay around
/// for a while. A setting should be picked wisely based on the network's reliability to deliver
/// and order messages and security requirements.
#[derive(Debug)]
pub struct DecryptionRatchet;

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "test_utils"), derive(Clone))]
pub struct DecryptionRatchetState {
    past_secrets: VecDeque<Option<Secret<MESSAGE_KEY_SIZE>>>,
    ratchet_head: RatchetSecretState,
}

impl DecryptionRatchet {
    pub fn init(secret: Secret<MESSAGE_KEY_SIZE>) -> DecryptionRatchetState {
        DecryptionRatchetState {
            past_secrets: VecDeque::new(),
            ratchet_head: RatchetSecret::init(secret),
        }
    }

    /// Returns a secret from the ratchet for decryption. Throws an error of requested secret is
    /// out of bounds.
    ///
    /// ## Limits Configuration
    ///
    /// - Out-of-order (ooo) tolerance:
    ///   This parameter defines a window for which decryption secrets are kept. This is useful in
    ///   case the ratchet cannot guarantee that all application messages have total order within
    ///   an epoch. Use this carefully, since keeping decryption secrets affects forward secrecy
    ///   within an epoch.
    /// - Maximum forward distance:
    ///   This parameter defines how many incoming messages can be skipped. This is useful if the
    ///   DS drops application messages.
    pub fn secret_for_decryption(
        mut y: DecryptionRatchetState,
        generation: Generation,
        maximum_forward_distance: u64,
        ooo_tolerance: u64,
    ) -> Result<(DecryptionRatchetState, Secret<MESSAGE_KEY_SIZE>), RatchetError> {
        let generation_head = y.ratchet_head.generation;

        // If generation is too distant in the future.
        if generation_head < u64::MAX - maximum_forward_distance
            && generation > generation_head + maximum_forward_distance
        {
            return Err(RatchetError::TooDistantInTheFuture);
        }

        // If generation id too distant in the past.
        if generation < generation_head && (generation_head - generation) > ooo_tolerance {
            return Err(RatchetError::TooDistantInThePast);
        }

        // If generation is the one the ratchet is currently at (regular case) or in the future.
        if generation >= generation_head {
            // Ratchet the chain forward as far as necessary.
            for _ in 0..(generation - generation_head) {
                // Derive the key material.
                let (y_ratchet_head_i, _, ratchet_secrets) =
                    RatchetSecret::ratchet_forward(y.ratchet_head)?;
                y.ratchet_head = y_ratchet_head_i;
                // Add it to the front of the queue.
                y.past_secrets.push_front(Some(ratchet_secrets));
            }
            let (y_ratchet_head_i, _, ratchet_secrets) =
                RatchetSecret::ratchet_forward(y.ratchet_head)?;
            y.ratchet_head = y_ratchet_head_i;
            // Add an entry to the past secrets queue to keep indexing consistent.
            y.past_secrets.push_front(None);
            // Remove persisted keys until it is within the bounds determined by the config.
            y.past_secrets.truncate(ooo_tolerance as usize);
            Ok((y, ratchet_secrets))
        } else {
            // If the requested generation is within the window of past secrets, we should get a
            // positive index.
            let window_index = (generation_head - generation) - 1;
            // We might not have the key material (e.g. we might have discarded it when generating
            // an encryption secret).
            let index = if window_index >= 0 {
                window_index
            } else {
                return Err(RatchetError::TooDistantInThePast);
            };
            // Get the relevant secrets from the past secrets queue.
            let ratchet_secrets = y
                .past_secrets
                .get_mut(index as usize)
                .ok_or(RatchetError::IndexOutOfBounds)?
                // We use take here to replace the entry in the `past_secrets` with `None`, thus
                // achieving FS for that secret as soon as the caller of this function drops it.
                .take()
                // If the requested generation was used to decrypt a message earlier, throw an
                // error.
                .ok_or(RatchetError::SecretReuse)?;
            Ok((y, ratchet_secrets))
        }
    }
}

#[derive(Debug, Error)]
pub enum RatchetError {
    #[error(transparent)]
    Hkdf(#[from] HkdfError),

    #[error("generation for message ratchet is too far into the future")]
    TooDistantInTheFuture,

    #[error("generation for message ratchet is too far into the past")]
    TooDistantInThePast,

    #[error("unknown message secret")]
    IndexOutOfBounds,

    #[error("tried to re-use secret for same generation")]
    SecretReuse,
}
