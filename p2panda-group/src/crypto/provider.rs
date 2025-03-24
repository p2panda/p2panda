// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Mutex;

use rand_chacha::rand_core::{SeedableRng, TryRngCore};
use thiserror::Error;

use crate::crypto::{aead, ed25519, hkdf, hpke, x25519, xchacha20, xeddsa};
use crate::traits::{CryptoProvider, RandProvider, XCryptoProvider};

/// Default implementation of the cryptographic algorithms which can be considered the "reference
/// implementation" of p2panda's group encryption.
///
/// "Basic" Algorithms:
/// - DHKEM-X25519 HPKE
/// - SHA256 HKDF
/// - ChaCha20Poly1305 AEAD
/// - Ed25519 (SHA512) DSA
///
/// "Extended" Algorithms:
/// - XEdDSA (DSA with X25519)
/// - XChaCha20Poly1305 (large IVs)
///
/// Random Number Generator:
/// - ChaCha20 stream cipher, seeded via `getrandom`
///
/// Most of the implementations use the [`libcrux`](https://github.com/cryspen/libcrux) crate
/// internally.
#[derive(Debug)]
pub struct Crypto {
    rng: Mutex<rand_chacha::ChaCha20Rng>,
}

impl Default for Crypto {
    fn default() -> Self {
        Self {
            rng: Mutex::new(rand_chacha::ChaCha20Rng::from_os_rng()),
        }
    }
}

#[cfg(test)]
impl Crypto {
    pub fn from_seed(seed: [u8; 32]) -> Self {
        Self {
            rng: Mutex::new(rand_chacha::ChaCha20Rng::from_seed(seed)),
        }
    }
}

impl CryptoProvider for Crypto {
    type Error = CryptoError<Self>;

    type AeadNonce = aead::AeadNonce;

    type AeadKey = aead::AeadKey;

    type PublicKey = x25519::PublicKey;

    type SecretKey = x25519::SecretKey;

    type HpkeCiphertext = hpke::HpkeCiphertext;

    type SigningKey = ed25519::SigningKey;

    type VerifyingKey = ed25519::VerifyingKey;

    type Signature = ed25519::Signature;

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

    fn sign(
        &self,
        bytes: &[u8],
        signing_key: &Self::SigningKey,
    ) -> Result<Self::Signature, Self::Error> {
        let signature = signing_key.sign(bytes)?;
        Ok(signature)
    }

    fn verify(
        &self,
        bytes: &[u8],
        verifying_key: &Self::VerifyingKey,
        signature: &Self::Signature,
    ) -> Result<(), Self::Error> {
        verifying_key.verify(bytes, signature)?;
        Ok(())
    }
}

impl XCryptoProvider for Crypto {
    type Error = XCryptoError<Self>;

    type XAeadNonce = xchacha20::XAeadNonce;

    type XAeadKey = xchacha20::XAeadKey;

    type XSigningKey = x25519::SecretKey;

    type XVerifyingKey = x25519::PublicKey;

    type XSignature = xeddsa::XSignature;

    fn x_aead_encrypt(
        &self,
        key: &Self::XAeadKey,
        plaintext: &[u8],
        nonce: Self::XAeadNonce,
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, Self::Error> {
        let ciphertext = xchacha20::x_aead_encrypt(key, plaintext, nonce, aad)?;
        Ok(ciphertext)
    }

    fn x_aead_decrypt(
        &self,
        key: &Self::XAeadKey,
        ciphertext_tag: &[u8],
        nonce: Self::XAeadNonce,
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, Self::Error> {
        let plaintext = xchacha20::x_aead_decrypt(key, ciphertext_tag, nonce, aad)?;
        Ok(plaintext)
    }

    fn x_sign(
        &self,
        bytes: &[u8],
        signing_key: &Self::XSigningKey,
    ) -> Result<Self::XSignature, Self::Error> {
        let signature = xeddsa::xeddsa_sign(bytes, signing_key, self)?;
        Ok(signature)
    }

    fn x_verify(
        &self,
        bytes: &[u8],
        verifying_key: &Self::XVerifyingKey,
        signature: &Self::XSignature,
    ) -> Result<(), Self::Error> {
        xeddsa::xeddsa_verify(bytes, verifying_key, signature)?;
        Ok(())
    }

    fn x_calculate_agreement(
        &self,
        secret_key: &Self::XSigningKey,
        public_key: &Self::XVerifyingKey,
    ) -> Result<Vec<u8>, Self::Error> {
        let agreement = secret_key.calculate_agreement(public_key)?;
        Ok(agreement)
    }
}

impl RandProvider for Crypto {
    type Error = RandError;

    fn random_array<const N: usize>(&self) -> Result<[u8; N], Self::Error> {
        let mut rng = self.rng.lock().map_err(|_| RandError::LockPoisoned)?;
        let mut out = [0u8; N];
        rng.try_fill_bytes(&mut out)
            .map_err(|_| RandError::NotEnoughRandomness)?;
        Ok(out)
    }

    fn random_vec(&self, len: usize) -> Result<Vec<u8>, Self::Error> {
        let mut rng = self.rng.lock().map_err(|_| RandError::LockPoisoned)?;
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

    #[error(transparent)]
    Signature(#[from] ed25519::SignatureError),
}

#[derive(Debug, Error)]
pub enum XCryptoError<RNG: RandProvider> {
    #[error(transparent)]
    XAead(#[from] xchacha20::XAeadError),

    #[error(transparent)]
    XEdDSA(#[from] xeddsa::XEdDSAError<RNG>),

    #[error(transparent)]
    X25519(#[from] x25519::X25519Error),
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
    use crate::traits::RandProvider;

    use super::Crypto;

    #[test]
    fn deterministic_randomness() {
        let sample_1 = {
            let rng = Crypto::from_seed([1; 32]);
            rng.random_vec(128).unwrap()
        };

        let sample_2 = {
            let rng = Crypto::from_seed([1; 32]);
            rng.random_vec(128).unwrap()
        };

        assert_eq!(sample_1, sample_2);
    }
}
