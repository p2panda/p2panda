// SPDX-License-Identifier: MIT OR Apache-2.0

//! Cryptographic algorithms and secure random number provider for `p2panda-group`.
//!
//! Following algorithms are used:
//! * ChaCha random number generator with 20 rounds
//! * AES-256-GCM AEAD
use std::sync::RwLock;

use rand_chacha::rand_core::{SeedableRng, TryRngCore};
use thiserror::Error;

use crate::crypto::traits::{CryptoProvider, RandProvider};
use crate::crypto::{aead, hkdf, hpke, x25519};

#[derive(Debug)]
pub struct Provider {
    rng: RwLock<rand_chacha::ChaCha20Rng>,
}

impl Default for Provider {
    fn default() -> Self {
        Self {
            rng: RwLock::new(rand_chacha::ChaCha20Rng::from_os_rng()),
        }
    }
}

#[cfg(test)]
impl Provider {
    pub fn from_seed(seed: [u8; 32]) -> Self {
        Self {
            rng: RwLock::new(rand_chacha::ChaCha20Rng::from_seed(seed)),
        }
    }
}

impl CryptoProvider for Provider {
    type Error = CryptoError<Self>;

    type AeadNonce = aead::AeadNonce;

    type AeadKey = aead::AeadKey;

    type PublicKey = x25519::PublicKey;

    type SecretKey = x25519::SecretKey;

    type HpkeCiphertext = hpke::HpkeCiphertext;

    fn aead_encrypt(
        &self,
        key: &Self::AeadKey,
        plaintext: &[u8],
        nonce: Self::AeadNonce,
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, Self::Error> {
        let ciphertext_tag = aead::aead_encrypt(key, plaintext, nonce, aad)?;
        Ok(ciphertext_tag)
    }

    fn aead_decrypt(
        &self,
        key: &Self::AeadKey,
        ciphertext_tag: &[u8],
        nonce: Self::AeadNonce,
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, Self::Error> {
        let plaintext = aead::aead_decrypt(key, ciphertext_tag, nonce, aad)?;
        Ok(plaintext)
    }

    fn hkdf<const N: usize>(
        &self,
        salt: &[u8],
        ikm: &[u8],
        info: Option<&[u8]>,
    ) -> Result<[u8; N], Self::Error> {
        let key_material = hkdf::hkdf(salt, ikm, info)?;
        Ok(key_material)
    }

    fn hpke_seal(
        &self,
        public_key: &Self::PublicKey,
        info: Option<&[u8]>,
        aad: Option<&[u8]>,
        plaintext: &[u8],
    ) -> Result<Self::HpkeCiphertext, Self::Error> {
        let ciphertext = hpke::hpke_seal(public_key, info, aad, plaintext, self)?;
        Ok(ciphertext)
    }

    fn hpke_open(
        &self,
        input: &Self::HpkeCiphertext,
        secret_key: &Self::SecretKey,
        info: Option<&[u8]>,
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, Self::Error> {
        let plaintext = hpke::hpke_open(input, secret_key, info, aad)?;
        Ok(plaintext)
    }
}

impl RandProvider for Provider {
    type Error = RandError;

    fn random_array<const N: usize>(&self) -> Result<[u8; N], Self::Error> {
        let mut rng = self.rng.write().map_err(|_| RandError::LockPoisoned)?;
        let mut out = [0u8; N];
        rng.try_fill_bytes(&mut out)
            .map_err(|_| RandError::NotEnoughRandomness)?;
        Ok(out)
    }

    fn random_vec(&self, len: usize) -> Result<Vec<u8>, Self::Error> {
        let mut rng = self.rng.write().map_err(|_| RandError::LockPoisoned)?;
        let mut out = vec![0u8; len];
        rng.try_fill_bytes(&mut out)
            .map_err(|_| RandError::NotEnoughRandomness)?;
        Ok(out)
    }
}

#[derive(Debug, Error)]
pub enum ProviderError<RNG: RandProvider> {
    #[error(transparent)]
    Crypto(#[from] CryptoError<RNG>),

    #[error(transparent)]
    Rand(#[from] RandError),
}

#[derive(Debug, Error)]
pub enum CryptoError<RNG: RandProvider> {
    #[error(transparent)]
    Aead(#[from] aead::AeadError),

    #[error(transparent)]
    Hkdf(#[from] hkdf::HkdfError),

    #[error(transparent)]
    Hpke(#[from] hpke::HpkeError<RNG>),
}

#[derive(Debug, Error)]
pub enum RandError {
    #[error("rng lock is poisoned")]
    LockPoisoned,

    #[error("unable to collect enough randomness")]
    NotEnoughRandomness,
}

#[cfg(test)]
mod tests {
    use crate::crypto::RandProvider;

    use super::Provider;

    #[test]
    fn deterministic_randomness() {
        let sample_1 = {
            let rng = Provider::from_seed([1; 32]);
            rng.random_vec(128).unwrap()
        };

        let sample_2 = {
            let rng = Provider::from_seed([1; 32]);
            rng.random_vec(128).unwrap()
        };

        assert_eq!(sample_1, sample_2);
    }
}
