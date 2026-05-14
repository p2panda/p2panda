// SPDX-License-Identifier: MIT OR Apache-2.0

//! Ed25519 key pairs and signatures.
//!
//! The `SigningKey` is used for creating digital signatures and the `VerifyingKey` is used for
//! verifying that a signature was indeed created by it's private counterpart. The private part of
//! a key pair is typically kept securely on one device and never transported, whereas the public
//! part acts as a peer's unique identifier and can be shared freely.
//!
//! ## Example
//!
//! ```
//! use p2panda_core::identity::SigningKey;
//!
//! let signing_key = SigningKey::generate();
//! let verifying_key = signing_key.verifying_key();
//!
//! let bytes: &[u8] = b"A very important message.";
//! let signature = signing_key.sign(bytes);
//!
//! assert!(verifying_key.verify(bytes, &signature))
//! ```
use std::fmt;
use std::hash::Hash as StdHash;
use std::str::FromStr;

#[cfg(feature = "arbitrary")]
use arbitrary::Arbitrary;
use ed25519_dalek::Signer;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The length of an Ed25519 `Signature`, in bytes.
pub const SIGNATURE_LEN: usize = ed25519_dalek::SIGNATURE_LENGTH;

/// The length of an Ed25519 `SigningKey`, in bytes.
pub const SIGNING_KEY_LEN: usize = ed25519_dalek::SECRET_KEY_LENGTH;

/// The length of an Ed25519 `VerifyingKey`, in bytes.
pub const VERIFYING_KEY_LEN: usize = ed25519_dalek::PUBLIC_KEY_LENGTH;

pub trait Author:
    Clone + PartialEq + Ord + StdHash + Serialize + for<'de> Deserialize<'de>
{
}

/// Private Ed25519 key used for digital signatures.
#[derive(Clone, Eq, PartialEq)]
pub struct SigningKey(ed25519_dalek::SigningKey);

impl Default for SigningKey {
    fn default() -> Self {
        Self::generate()
    }
}

impl SigningKey {
    /// Generates a new signing key using the system's random number generator (CSPRNG) as a seed.
    pub fn generate() -> Self {
        let mut csprng: OsRng = OsRng;
        let signing_key = ed25519_dalek::SigningKey::generate(&mut csprng);
        Self(signing_key)
    }

    /// Create a `SigningKey` from its raw bytes representation.
    pub fn from_bytes(bytes: &[u8; SIGNING_KEY_LEN]) -> Self {
        Self(ed25519_dalek::SigningKey::from_bytes(bytes))
    }

    /// Bytes of the signing key.
    pub fn as_bytes(&self) -> &[u8; SIGNING_KEY_LEN] {
        self.0.as_bytes()
    }

    /// Convert the signing key to a hex string.
    pub fn to_hex(&self) -> String {
        hex::encode(self.0.as_bytes())
    }

    /// Returns public key using this signing counterpart.
    pub fn verifying_key(&self) -> VerifyingKey {
        self.0.verifying_key().into()
    }

    /// Sign the provided bytestring using this signing key returning a digital signature.
    pub fn sign(&self, bytes: &[u8]) -> Signature {
        self.0.sign(bytes).into()
    }
}

impl fmt::Display for SigningKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

#[cfg(any(test, feature = "test_utils"))]
impl fmt::Debug for SigningKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("SigningKey")
            .field(self.0.as_bytes())
            .finish()
    }
}

#[cfg(not(any(test, feature = "test_utils")))]
impl fmt::Debug for SigningKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("SigningKey").field(&"***").finish()
    }
}

impl From<[u8; SIGNING_KEY_LEN]> for SigningKey {
    fn from(value: [u8; SIGNING_KEY_LEN]) -> Self {
        Self::from_bytes(&value)
    }
}

impl From<SigningKey> for [u8; SIGNING_KEY_LEN] {
    fn from(value: SigningKey) -> Self {
        *value.as_bytes()
    }
}

impl From<&[u8; SIGNING_KEY_LEN]> for SigningKey {
    fn from(value: &[u8; SIGNING_KEY_LEN]) -> Self {
        Self::from_bytes(value)
    }
}

impl TryFrom<&[u8]> for SigningKey {
    type Error = IdentityError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let value_len = value.len();

        let checked_value: [u8; SIGNING_KEY_LEN] = value
            .try_into()
            .map_err(|_| IdentityError::InvalidLength(value_len, SIGNING_KEY_LEN))?;

        Ok(Self::from(checked_value))
    }
}

#[cfg(feature = "arbitrary")]
impl<'a> Arbitrary<'a> for SigningKey {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let bytes = <[u8; SIGNING_KEY_LEN] as Arbitrary>::arbitrary(u)?;
        Ok(SigningKey::from_bytes(&bytes))
    }
}

/// Public Ed25519 key used for identifying peers and verifying signed data.
#[derive(Default, Hash, PartialEq, Eq, Copy, Clone)]
pub struct VerifyingKey(ed25519_dalek::VerifyingKey);

impl PartialOrd for VerifyingKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for VerifyingKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.to_hex().cmp(&other.to_hex())
    }
}

impl VerifyingKey {
    /// Create a `VerifyingKey` from its raw bytes representation.
    pub fn from_bytes(bytes: &[u8; VERIFYING_KEY_LEN]) -> Result<Self, IdentityError> {
        Ok(Self(ed25519_dalek::VerifyingKey::from_bytes(bytes)?))
    }

    /// Bytes of the public key.
    pub fn as_bytes(&self) -> &[u8; VERIFYING_KEY_LEN] {
        self.0.as_bytes()
    }

    /// Convert the public key to a hex string.
    pub fn to_hex(&self) -> String {
        hex::encode(self.0.as_bytes())
    }

    /// Verify a signature over a byte slice with this public key.
    pub fn verify(&self, bytes: &[u8], signature: &Signature) -> bool {
        self.0.verify_strict(bytes, &signature.0).is_ok()
    }
}

impl fmt::Display for VerifyingKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl fmt::Debug for VerifyingKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("VerifyingKey")
            .field(self.0.as_bytes())
            .finish()
    }
}

impl From<VerifyingKey> for ed25519_dalek::VerifyingKey {
    fn from(value: VerifyingKey) -> Self {
        value.0
    }
}

impl From<ed25519_dalek::VerifyingKey> for VerifyingKey {
    fn from(value: ed25519_dalek::VerifyingKey) -> Self {
        Self(value)
    }
}

impl TryFrom<[u8; VERIFYING_KEY_LEN]> for VerifyingKey {
    type Error = IdentityError;

    fn try_from(value: [u8; VERIFYING_KEY_LEN]) -> Result<Self, Self::Error> {
        Self::from_bytes(&value)
    }
}

impl From<VerifyingKey> for [u8; VERIFYING_KEY_LEN] {
    fn from(value: VerifyingKey) -> Self {
        *value.as_bytes()
    }
}

impl TryFrom<&[u8; VERIFYING_KEY_LEN]> for VerifyingKey {
    type Error = IdentityError;

    fn try_from(value: &[u8; VERIFYING_KEY_LEN]) -> Result<Self, Self::Error> {
        Self::from_bytes(value)
    }
}

impl TryFrom<&[u8]> for VerifyingKey {
    type Error = IdentityError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let value_len = value.len();

        let checked_value: [u8; VERIFYING_KEY_LEN] = value
            .try_into()
            .map_err(|_| IdentityError::InvalidLength(value_len, VERIFYING_KEY_LEN))?;

        Self::try_from(checked_value)
    }
}

impl FromStr for VerifyingKey {
    type Err = IdentityError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(hex::decode(value)?.as_slice())
    }
}

impl Author for VerifyingKey {}

#[cfg(feature = "arbitrary")]
impl<'a> Arbitrary<'a> for VerifyingKey {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let bytes = <[u8; VERIFYING_KEY_LEN] as Arbitrary>::arbitrary(u)?;
        let verifying_key =
            VerifyingKey::from_bytes(&bytes).map_err(|_| arbitrary::Error::IncorrectFormat)?;
        Ok(verifying_key)
    }
}

/// Ed25519 signature.
#[derive(Copy, Eq, PartialEq, Clone)]
pub struct Signature(ed25519_dalek::Signature);

impl Signature {
    /// Create a `Signature` from its raw bytes representation.
    pub fn from_bytes(bytes: &[u8; SIGNATURE_LEN]) -> Self {
        Self(ed25519_dalek::Signature::from_bytes(bytes))
    }

    /// Bytes of the signature.
    pub fn to_bytes(&self) -> [u8; SIGNATURE_LEN] {
        let mut ret = [0u8; SIGNATURE_LEN];
        let (r, s) = ret.split_at_mut(32);
        r.copy_from_slice(self.0.r_bytes());
        s.copy_from_slice(self.0.s_bytes());
        ret
    }

    /// Convert the signature to a hex string.
    pub fn to_hex(&self) -> String {
        hex::encode(self.to_bytes())
    }
}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Signature").field(&self.to_bytes()).finish()
    }
}

impl FromStr for Signature {
    type Err = IdentityError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(hex::decode(value)?.as_slice())
    }
}

impl From<Signature> for ed25519_dalek::Signature {
    fn from(value: Signature) -> Self {
        value.0
    }
}

impl From<ed25519_dalek::Signature> for Signature {
    fn from(value: ed25519_dalek::Signature) -> Self {
        Self(value)
    }
}

impl From<[u8; SIGNATURE_LEN]> for Signature {
    fn from(value: [u8; SIGNATURE_LEN]) -> Self {
        Self::from_bytes(&value)
    }
}

impl From<&[u8; SIGNATURE_LEN]> for Signature {
    fn from(value: &[u8; SIGNATURE_LEN]) -> Self {
        Self::from_bytes(value)
    }
}

impl TryFrom<&[u8]> for Signature {
    type Error = IdentityError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let value_len = value.len();

        let checked_value: [u8; SIGNATURE_LEN] = value
            .try_into()
            .map_err(|_| IdentityError::InvalidLength(value_len, SIGNATURE_LEN))?;

        Ok(Self::from(checked_value))
    }
}

#[cfg(feature = "arbitrary")]
impl<'a> Arbitrary<'a> for Signature {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let bytes = <[u8; SIGNATURE_LEN] as Arbitrary>::arbitrary(u)?;
        Ok(Signature::from_bytes(&bytes))
    }
}

#[derive(Error, Debug)]
pub enum IdentityError {
    /// Invalid number of bytes.
    #[error("invalid bytes length of {0}, expected {1} bytes")]
    InvalidLength(usize, usize),

    /// String contains invalid hexadecimal characters.
    #[error("invalid hex encoding in string")]
    InvalidHexEncoding(#[from] hex::FromHexError),

    /// Errors which may occur while processing signatures and key pairs.
    ///
    /// This error may arise due to:
    ///
    /// * Being given bytes with a length different to what was expected.
    ///
    /// * A problem decompressing `r`, a curve point, in the `Signature`, or the curve point for a
    ///   `VerifyingKey`.
    ///
    /// * Failure of a signature to satisfy the verification equation.
    #[error("invalid signature: {0}")]
    InvalidSignature(#[from] ed25519_dalek::SignatureError),
}

#[cfg(test)]
mod tests {
    use super::SigningKey;

    #[test]
    fn signing() {
        let signing_key = SigningKey::generate();
        let verifying_key = signing_key.verifying_key();
        let bytes = b"test";
        let signature = signing_key.sign(bytes);
        assert!(verifying_key.verify(bytes, &signature));

        // Invalid data
        assert!(!verifying_key.verify(b"not test", &signature));

        // Invalid public key
        let verifying_key_2 = SigningKey::generate().verifying_key();
        assert!(!verifying_key_2.verify(bytes, &signature));
    }
}
