// SPDX-License-Identifier: MIT OR Apache-2.0

//! Elliptic-curve Diffie–Hellman (ECDH) key agreement scheme (X25519).
use libcrux::ecdh::Algorithm;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use zeroize::ZeroizeOnDrop;

const ALGORITHM: Algorithm = Algorithm::X25519;

pub const SECRET_KEY_SIZE: usize = 32;
pub const PUBLIC_KEY_SIZE: usize = 32;
pub const AGREEMENT_SIZE: usize = 32;

#[derive(Clone, Debug, Serialize, Deserialize, ZeroizeOnDrop)]
pub struct SecretKey([u8; SECRET_KEY_SIZE]);

impl SecretKey {
    pub fn from_bytes(bytes: [u8; SECRET_KEY_SIZE]) -> Self {
        // Clamping
        let mut bytes = bytes;
        bytes[0] &= 248u8;
        bytes[31] &= 127u8;
        bytes[31] |= 64u8;
        SecretKey(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; SECRET_KEY_SIZE] {
        &self.0
    }

    pub fn to_bytes(&self) -> [u8; SECRET_KEY_SIZE] {
        self.0
    }

    pub fn public_key(&self) -> Result<PublicKey, X25519Error> {
        let bytes = libcrux::ecdh::secret_to_public(ALGORITHM, self.0)
            .map_err(|_| X25519Error::InvalidCurve)?;
        Ok(PublicKey(
            bytes
                .try_into()
                .expect("correct public key size from ecdh method"),
        ))
    }

    pub fn calculate_agreement(
        &self,
        their_public: &PublicKey,
    ) -> Result<[u8; AGREEMENT_SIZE], X25519Error> {
        let shared_secret = libcrux::ecdh::derive(ALGORITHM, their_public.as_bytes(), self.0)
            .map_err(|_| X25519Error::InvalidCurve)?;
        Ok(shared_secret
            .try_into()
            .expect("correct shared secret size from ecdh method"))
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicKey([u8; PUBLIC_KEY_SIZE]);

impl PublicKey {
    pub fn from_bytes(public_key: [u8; PUBLIC_KEY_SIZE]) -> Self {
        Self(public_key)
    }

    pub fn as_bytes(&self) -> &[u8; PUBLIC_KEY_SIZE] {
        &self.0
    }

    pub fn to_bytes(self) -> [u8; PUBLIC_KEY_SIZE] {
        self.0
    }
}

#[derive(Debug, Error)]
pub enum X25519Error {
    #[error("invalid curve point or scalar")]
    InvalidCurve,
}

#[cfg(test)]
mod tests {
    use crate::crypto::{Provider, RandProvider};

    use super::SecretKey;

    #[test]
    fn diffie_hellmann() {
        let rng = Provider::default();

        let alice_secret_key = SecretKey::from_bytes(rng.random_array().unwrap());
        let alice_public_key = alice_secret_key.public_key().unwrap();

        let bob_secret_key = SecretKey::from_bytes(rng.random_array().unwrap());
        let bob_public_key = bob_secret_key.public_key().unwrap();

        let alice_shared_secret = alice_secret_key
            .calculate_agreement(&bob_public_key)
            .unwrap();
        let bob_shared_secret = bob_secret_key
            .calculate_agreement(&alice_public_key)
            .unwrap();

        assert_eq!(alice_shared_secret, bob_shared_secret);
    }
}
