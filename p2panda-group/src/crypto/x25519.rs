// SPDX-License-Identifier: MIT OR Apache-2.0

//! Elliptic-curve Diffie–Hellman (ECDH) key agreement scheme (X25519).
use std::fmt;

use libcrux_ecdh::Algorithm;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::secret::Secret;

const ALGORITHM: Algorithm = Algorithm::X25519;

/// 256-bit secret key size.
pub const SECRET_KEY_SIZE: usize = 32;

/// 256-bit public key size.
pub const PUBLIC_KEY_SIZE: usize = 32;

/// Secret Curve25519 key used for ECDH key agreement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretKey(Secret<SECRET_KEY_SIZE>);

impl SecretKey {
    // TODO: Remove this in later PRs.
    #[allow(dead_code)]
    pub(crate) fn from_bytes(bytes: [u8; SECRET_KEY_SIZE]) -> Self {
        // Clamping
        let mut bytes = bytes;
        bytes[0] &= 248u8;
        bytes[31] &= 127u8;
        bytes[31] |= 64u8;
        SecretKey(Secret::from_bytes(bytes))
    }

    pub(crate) fn as_bytes(&self) -> &[u8; SECRET_KEY_SIZE] {
        self.0.as_bytes()
    }

    pub fn public_key(&self) -> Result<PublicKey, X25519Error> {
        let bytes = libcrux_ecdh::secret_to_public(ALGORITHM, self.0.as_bytes())
            .map_err(|_| X25519Error::InvalidCurve)?;
        Ok(PublicKey(
            bytes
                .try_into()
                .expect("correct public key size from ecdh method"),
        ))
    }

    pub fn calculate_agreement(&self, their_public: &PublicKey) -> Result<Vec<u8>, X25519Error> {
        let shared_secret =
            libcrux_ecdh::derive(ALGORITHM, their_public.as_bytes(), self.0.as_bytes())
                .map_err(|_| X25519Error::InvalidCurve)?;
        Ok(shared_secret)
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

    pub fn to_hex(&self) -> String {
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
    use crate::crypto::Crypto;
    use crate::traits::RandProvider;

    use super::SecretKey;

    #[test]
    fn diffie_hellmann() {
        let rng = Crypto::from_seed([1; 32]);

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
