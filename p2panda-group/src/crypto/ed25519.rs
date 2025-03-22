// SPDX-License-Identifier: MIT OR Apache-2.0

//! Edwards-Curve Digital Signature Algorithm (EdDSA) related to Curve25519 using SHA-512.
use serde::{Deserialize, Serialize};
use thiserror::Error;
use zeroize::ZeroizeOnDrop;

pub const SIGNING_KEY_SIZE: usize = 32;
pub const VERIFYING_KEY_SIZE: usize = 32;
pub const SIGNATURE_SIZE: usize = 64;

#[derive(Clone, Serialize, Deserialize, ZeroizeOnDrop)]
pub struct SigningKey([u8; SIGNING_KEY_SIZE]);

impl SigningKey {
    pub fn from_bytes(bytes: [u8; SIGNING_KEY_SIZE]) -> Self {
        // Clamping
        let mut bytes = bytes;
        bytes[0] &= 248u8;
        bytes[31] &= 127u8;
        bytes[31] |= 64u8;
        SigningKey(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; SIGNING_KEY_SIZE] {
        &self.0
    }

    pub fn to_bytes(&self) -> [u8; SIGNING_KEY_SIZE] {
        self.0
    }

    pub fn verifying_key(&self) -> VerifyingKey {
        let mut bytes = [0u8; VERIFYING_KEY_SIZE];
        libcrux_ed25519::secret_to_public(&mut bytes, &self.0);
        VerifyingKey(bytes)
    }

    pub fn sign(&self, bytes: &[u8]) -> Result<Signature, SignatureError> {
        let bytes =
            libcrux_ed25519::sign(bytes, &self.0).map_err(|_| SignatureError::SigningFailed)?;
        Ok(Signature(bytes))
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyingKey([u8; VERIFYING_KEY_SIZE]);

impl VerifyingKey {
    pub fn from_bytes(bytes: [u8; VERIFYING_KEY_SIZE]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; VERIFYING_KEY_SIZE] {
        &self.0
    }

    pub fn to_bytes(self) -> [u8; VERIFYING_KEY_SIZE] {
        self.0
    }

    pub fn verify(&self, bytes: &[u8], signature: &Signature) -> Result<(), SignatureError> {
        libcrux_ed25519::verify(bytes, &self.0, &signature.0)
            .map_err(|_| SignatureError::VerificationFailed)?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signature([u8; SIGNATURE_SIZE]);

impl Signature {
    pub fn from_bytes(bytes: [u8; SIGNATURE_SIZE]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; SIGNATURE_SIZE] {
        &self.0
    }

    pub fn to_bytes(&self) -> [u8; SIGNATURE_SIZE] {
        self.0
    }
}

#[derive(Debug, Error)]
pub enum SignatureError {
    #[error("signature does not match public key and payload")]
    VerificationFailed,

    #[error("could not sign payload")]
    SigningFailed,
}

#[cfg(test)]
mod tests {
    use crate::crypto::Crypto;
    use crate::traits::RandProvider;

    use super::{SignatureError, SigningKey};

    #[test]
    fn sign_and_verify() {
        let rng = Crypto::from_seed([1; 32]);

        let signing_key = SigningKey::from_bytes(rng.random_array().unwrap());
        let verifying_key = signing_key.verifying_key();

        let signature = signing_key.sign(b"Hello, Panda!").unwrap();
        assert!(verifying_key.verify(b"Hello, Panda!", &signature).is_ok());
    }

    #[test]
    fn failed_verify() {
        let rng = Crypto::from_seed([1; 32]);

        let signing_key = SigningKey::from_bytes(rng.random_array().unwrap());
        let verifying_key = signing_key.verifying_key();
        let signature = signing_key.sign(b"Hello, Panda!").unwrap();

        let invalid_signing_key = SigningKey::from_bytes(rng.random_array().unwrap());
        let invalid_verifying_key = invalid_signing_key.verifying_key();
        let invalid_signature = invalid_signing_key.sign(b"Hello, Panda!").unwrap();

        assert_ne!(verifying_key, invalid_verifying_key);
        assert_ne!(signature, invalid_signature);

        assert!(matches!(
            verifying_key.verify(b"Invalid Data", &signature),
            Err(SignatureError::VerificationFailed)
        ));
        assert!(matches!(
            invalid_verifying_key.verify(b"Hello, Panda!", &signature),
            Err(SignatureError::VerificationFailed)
        ));
        assert!(matches!(
            verifying_key.verify(b"Hello, Panda!", &invalid_signature),
            Err(SignatureError::VerificationFailed)
        ));
    }
}
