// SPDX-License-Identifier: MIT OR Apache-2.0

//! Encryption and decryption ratches with lost or out-of-order messages.
use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto::Secret;
use crate::crypto::aead::AeadNonce;
use crate::crypto::hkdf::{HkdfError, hkdf};

/// 256-bit secret message key.
pub const MESSAGE_KEY_SIZE: usize = 32;

/// Secret message key.
pub type RatchetKey = Secret<MESSAGE_KEY_SIZE>;

/// AEAD nonce to encrypt message.
pub type RatchetNonce = AeadNonce;

/// AEAD parameters to encrypt message.
pub type RatchetKeyMaterial = (RatchetKey, RatchetNonce);

/// Key generation of message ratchet.
pub type Generation = u32;

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
    ) -> Result<(RatchetSecretState, Generation, RatchetKeyMaterial), RatchetError> {
        let generation = y.generation;

        // Derive key material from current secret.
        let nonce: AeadNonce = hkdf(b"nonce", y.secret.as_bytes(), None)?;
        let key: [u8; MESSAGE_KEY_SIZE] = hkdf(b"key", y.secret.as_bytes(), None)?;

        // Ratchet forward.
        y.generation += 1;
        y.secret = Secret::from_bytes(hkdf(b"chain", y.secret.as_bytes(), None)?);

        Ok((y, generation, (Secret::from_bytes(key), nonce)))
    }
}

/// Message ratchet for decryption with support for handling lost or out-of-order messages.
///
/// ## Out-of-order handling
///
/// Out-of-order messages cause the ratchet to "jump" ahead and keep "unused" keys persisted until
/// they're used eventually.
///
/// In this example our chain has a length of 2 at the moment a message for generation 4 arrives
/// out of order (we've expected generation 2). Now we pre-generate the keys for the "jumped"
/// messages (generation 2 and 3) and keep them persisted for later. We decrypt the new message for
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
/// limits has implications for the forward secrecy of an application as unused keys stay around
/// for a while. A setting should be picked wisely based on the network's reliability to deliver
/// and order messages and security requirements.
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

    /// Returns a secret from the ratchet for decryption. Throws an error if requested secret is
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
    ///   application drops messages.
    pub fn secret_for_decryption(
        mut y: DecryptionRatchetState,
        generation: Generation,
        maximum_forward_distance: u32,
        ooo_tolerance: u32,
    ) -> Result<(DecryptionRatchetState, RatchetKeyMaterial), RatchetError> {
        let generation_head = y.ratchet_head.generation;

        // If generation is too distant in the future.
        if generation_head < u32::MAX - maximum_forward_distance
            && generation > generation_head + maximum_forward_distance
        {
            return Err(RatchetError::TooDistantInTheFuture);
        }

        // If generation is too distant in the past.
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
            let window_index = ((generation_head - generation) as i32) - 1;
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

#[cfg(test)]
mod tests {
    use crate::Rng;
    use crate::crypto::Secret;

    use super::{DecryptionRatchet, MESSAGE_KEY_SIZE, RatchetError, RatchetSecret};

    #[test]
    fn ratchet_forward() {
        let rng = Rng::from_seed([1; 32]);

        let update_secret = Secret::from_bytes(rng.random_array::<MESSAGE_KEY_SIZE>().unwrap());

        let ratchet = RatchetSecret::init(update_secret);
        let (ratchet, generation, secret_0) = RatchetSecret::ratchet_forward(ratchet).unwrap();
        assert_eq!(generation, 0);
        assert_eq!(ratchet.generation, 1);

        let (ratchet, generation, secret_1) = RatchetSecret::ratchet_forward(ratchet).unwrap();
        assert_eq!(generation, 1);
        assert_eq!(ratchet.generation, 2);

        // Ratchet secrets do not match across generations.
        assert_ne!(secret_0, secret_1);
    }

    #[test]
    fn forward_secrecy() {
        let rng = Rng::from_seed([1; 32]);

        let update_secret = Secret::from_bytes(rng.random_array::<MESSAGE_KEY_SIZE>().unwrap());

        let ooo_tolerance = 4;
        let max_forward = 100;

        let ratchet = DecryptionRatchet::init(update_secret);

        let (ratchet, secret) =
            DecryptionRatchet::secret_for_decryption(ratchet, 0, max_forward, ooo_tolerance)
                .unwrap();

        // Generation should have increased.
        assert_eq!(ratchet.ratchet_head.generation, 1);

        // No secrets have been kept.
        assert_ne!(ratchet.ratchet_head.secret, secret.0);
        assert!(!ratchet.past_secrets.iter().any(|secret| secret.is_some()));

        // Re-trying to retreive the secret for the same generation should fail.
        assert!(matches!(
            DecryptionRatchet::secret_for_decryption(
                ratchet.clone(),
                0,
                max_forward,
                ooo_tolerance
            ),
            Err(RatchetError::SecretReuse),
        ));

        // Move the ratchet forwards a few generations.
        let jump = 10;
        let (mut ratchet, _) =
            DecryptionRatchet::secret_for_decryption(ratchet, jump, max_forward, ooo_tolerance)
                .unwrap();

        // Now let's get a few keys. The first time we're trying to get the key of a given
        // generation, it should work. The second time, we should get an error.
        for generation in jump - ooo_tolerance + 1..jump {
            let (ratchet_i, _) = DecryptionRatchet::secret_for_decryption(
                ratchet,
                generation,
                max_forward,
                ooo_tolerance,
            )
            .unwrap();

            assert!(matches!(
                DecryptionRatchet::secret_for_decryption(
                    ratchet_i.clone(),
                    generation,
                    max_forward,
                    ooo_tolerance
                ),
                Err(RatchetError::SecretReuse),
            ));

            ratchet = ratchet_i;
        }

        // No secrets have been kept.
        assert!(!ratchet.past_secrets.iter().any(|secret| secret.is_some()));
    }

    #[test]
    fn out_of_order() {
        let rng = Rng::from_seed([1; 32]);

        let update_secret = Secret::from_bytes(rng.random_array::<MESSAGE_KEY_SIZE>().unwrap());

        let max_forward = 3;
        let ooo_tolerance = 3;

        let alice = RatchetSecret::init(update_secret.clone());
        let bob = DecryptionRatchet::init(update_secret);

        let (alice, _, alice_secret_0) = RatchetSecret::ratchet_forward(alice).unwrap();
        let (alice, _, _alice_secret_1) = RatchetSecret::ratchet_forward(alice).unwrap();
        let (alice, _, alice_secret_2) = RatchetSecret::ratchet_forward(alice).unwrap();
        let (alice, _, alice_secret_3) = RatchetSecret::ratchet_forward(alice).unwrap();
        let (alice, _, alice_secret_4) = RatchetSecret::ratchet_forward(alice).unwrap();
        assert_eq!(alice.generation, 5);

        // Bob derives the first secret for Alice's first message.
        let (bob, bob_secret_0) =
            DecryptionRatchet::secret_for_decryption(bob, 0, max_forward, ooo_tolerance).unwrap();
        assert_eq!(alice_secret_0, bob_secret_0);

        // Alice's messages arrive out-of-order, Bob derives them still.
        let (bob, bob_secret_4) =
            DecryptionRatchet::secret_for_decryption(bob, 4, max_forward, ooo_tolerance).unwrap();
        assert_eq!(alice_secret_4, bob_secret_4);
        assert_eq!(bob.ratchet_head.generation, 5);
        let (bob, bob_secret_3) =
            DecryptionRatchet::secret_for_decryption(bob, 3, max_forward, ooo_tolerance).unwrap();
        assert_eq!(alice_secret_3, bob_secret_3);
        let (bob, bob_secret_2) =
            DecryptionRatchet::secret_for_decryption(bob, 2, max_forward, ooo_tolerance).unwrap();
        assert_eq!(alice_secret_2, bob_secret_2);

        // Alice's message from generation 1 arrives, but it's already outside of the tolerance
        // window, we expect an error here.
        assert!(matches!(
            DecryptionRatchet::secret_for_decryption(bob.clone(), 1, max_forward, ooo_tolerance),
            Err(RatchetError::TooDistantInThePast)
        ));

        // Bob receives a message very far into the future from Alice, but this is also outside the
        // tolerated window, we expect an error here.
        assert!(matches!(
            DecryptionRatchet::secret_for_decryption(
                bob.clone(),
                bob.ratchet_head.generation + max_forward + 1,
                max_forward,
                ooo_tolerance
            ),
            Err(RatchetError::TooDistantInTheFuture)
        ));
    }
}
