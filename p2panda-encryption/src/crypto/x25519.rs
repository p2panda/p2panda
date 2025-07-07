// SPDX-License-Identifier: MIT OR Apache-2.0

//! Elliptic-curve Diffie–Hellman (ECDH) key agreement scheme (X25519).
use std::fmt;

use curve25519_dalek::scalar::clamp_integer;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto::Secret;

/// 256-bit secret key size.
pub const SECRET_KEY_SIZE: usize = 32;

/// 256-bit public key size.
pub const PUBLIC_KEY_SIZE: usize = 32;

/// 256-bit shared secret size.
pub const SHARED_SECRET_SIZE: usize = 32;

/// Secret Curve25519 key used for ECDH key agreement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretKey(Secret<SECRET_KEY_SIZE>);

impl SecretKey {
    #[cfg(not(feature = "test_utils"))]
    pub(crate) fn from_bytes(bytes: [u8; SECRET_KEY_SIZE]) -> Self {
        SecretKey(Secret::from_bytes(clamp_integer(bytes)))
    }

    #[cfg(feature = "test_utils")]
    pub fn from_bytes(bytes: [u8; SECRET_KEY_SIZE]) -> Self {
        SecretKey(Secret::from_bytes(clamp_integer(bytes)))
    }

    pub(crate) fn as_bytes(&self) -> &[u8; SECRET_KEY_SIZE] {
        self.0.as_bytes()
    }

    pub fn public_key(&self) -> Result<PublicKey, X25519Error> {
        let static_secret = x25519_dalek::StaticSecret::from(*self.0.as_bytes());
        let public_key = x25519_dalek::PublicKey::from(&static_secret);
        Ok(PublicKey(public_key.to_bytes()))
    }

    pub fn calculate_agreement(
        &self,
        their_public: &PublicKey,
    ) -> Result<[u8; SHARED_SECRET_SIZE], X25519Error> {
        let static_secret = x25519_dalek::StaticSecret::from(*self.0.as_bytes());
        let shared_secret =
            static_secret.diffie_hellman(&x25519_dalek::PublicKey::from(their_public.to_bytes()));
        Ok(shared_secret.to_bytes())
    }
}

/// Public Curve25519 key used for ECDH key agreement.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicKey(#[serde(with = "serde_bytes")] [u8; PUBLIC_KEY_SIZE]);

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

    pub fn to_hex(self) -> String {
        hex::encode(self.as_bytes())
    }
}

impl fmt::Display for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

#[derive(Debug, Error)]
pub enum X25519Error {
    #[error("invalid curve point or scalar")]
    InvalidCurve,
}

#[cfg(test)]
mod tests {
    use crate::crypto::Rng;

    use super::SecretKey;

    #[test]
    fn diffie_hellmann() {
        let rng = Rng::from_seed([1; 32]);

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
