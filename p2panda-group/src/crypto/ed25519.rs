// SPDX-License-Identifier: MIT OR Apache-2.0

//! Edwards-Curve Digital Signature Algorithm (EdDSA) related to Curve25519 using SHA-512.
//!
//! <https://www.rfc-editor.org/rfc/rfc8032>
use std::fmt;

use curve25519_dalek::scalar::clamp_integer;
use ed25519_dalek::ed25519::signature::SignerMut;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto::Secret;

/// 256-bit signing key.
pub const SIGNING_KEY_SIZE: usize = 32;

/// 256-bit verifying key.
pub const VERIFYING_KEY_SIZE: usize = 32;

/// 512-bit signature.
pub const SIGNATURE_SIZE: usize = 64;

/// Ed25519 signing key which can be used to produce signatures.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SigningKey(Secret<SIGNING_KEY_SIZE>);

impl SigningKey {
    // TODO: Remove this in later PRs.
    #[allow(dead_code)]
    pub(crate) fn from_bytes(bytes: [u8; SIGNING_KEY_SIZE]) -> Self {
        SigningKey(Secret::from_bytes(clamp_integer(bytes)))
    }

    // TODO: Remove this in later PRs.
    #[allow(dead_code)]
    pub(crate) fn as_bytes(&self) -> &[u8; SIGNING_KEY_SIZE] {
        self.0.as_bytes()
    }

    /// Get the [`VerifyingKey`] for this [`SigningKey`].
    pub fn verifying_key(&self) -> VerifyingKey {
        let secret_key = ed25519_dalek::SigningKey::from_bytes(self.0.as_bytes());
        VerifyingKey(secret_key.verifying_key().to_bytes())
    }

    /// Sign the provided data using returning a digital signature.
    pub fn sign(&self, bytes: &[u8]) -> Result<Signature, SignatureError> {
        let mut secret_key = ed25519_dalek::SigningKey::from_bytes(self.0.as_bytes());
        let signature = secret_key.sign(bytes);
        Ok(Signature(signature.to_bytes()))
    }
}

/// An Ed25519 public key.
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

    pub fn to_hex(self) -> String {
        hex::encode(self.as_bytes())
    }

    /// Verify a signature on provided data with this signing key's public key.
    pub fn verify(&self, bytes: &[u8], signature: &Signature) -> Result<(), SignatureError> {
        let dalek_signature = ed25519_dalek::Signature::from_bytes(signature.as_bytes());
        let dalek_verifying = ed25519_dalek::VerifyingKey::from_bytes(self.as_bytes())?;
        dalek_verifying
            .verify_strict(bytes, &dalek_signature)
            .map_err(|_| SignatureError::VerificationFailed)?;
        Ok(())
    }
}

impl fmt::Display for VerifyingKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// Ed25519 signature.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature(#[serde(with = "serde_bytes")] [u8; SIGNATURE_SIZE]);

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

    pub fn to_hex(&self) -> String {
        hex::encode(self.as_bytes())
    }
}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

#[derive(Debug, Error)]
pub enum SignatureError {
    #[error("signature does not match public key and payload")]
    VerificationFailed,

    #[error(transparent)]
    Ed25519Dalek(#[from] ed25519_dalek::SignatureError),
}

#[cfg(test)]
mod tests {
    use crate::crypto::Rng;

    use super::{SignatureError, SigningKey};

    #[test]
    fn sign_and_verify() {
        let rng = Rng::from_seed([1; 32]);

        let signing_key = SigningKey::from_bytes(rng.random_array().unwrap());
        let verifying_key = signing_key.verifying_key();

        let signature = signing_key.sign(b"Hello, Panda!").unwrap();
        assert!(verifying_key.verify(b"Hello, Panda!", &signature).is_ok());
    }

    #[test]
    fn failed_verify() {
        let rng = Rng::from_seed([1; 32]);

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
