// SPDX-License-Identifier: MIT OR Apache-2.0

//! Ed25519 key pairs and signatures.
//!
//! The `PrivateKey` is used for creating digital signatures and the `PublicKey` is used for
//! verifying that a signature was indeed created by it's private counterpart. The private part of
//! a key pair is typically kept securely on one device and never transported, whereas the public
//! part acts as a peer's unique identifier and can be shared freely.
//!
//! ## Example
//!
//! ```
//! use p2panda_core::identity::PrivateKey;
//!
//! let private_key = PrivateKey::new();
//! let public_key = private_key.public_key();
//!
//! let bytes: &[u8] = b"A very important message.";
//! let signature = private_key.sign(bytes);
//!
//! assert!(public_key.verify(bytes, &signature))
//! ```
use std::fmt;
use std::str::FromStr;

#[cfg(feature = "arbitrary")]
use arbitrary::Arbitrary;
use ed25519_dalek::Signer;
use rand::rngs::OsRng;
use thiserror::Error;

/// The length of an Ed25519 `Signature`, in bytes.
pub const SIGNATURE_LEN: usize = ed25519_dalek::SIGNATURE_LENGTH;

/// The length of an Ed25519 `PrivateKey`, in bytes.
pub const PRIVATE_KEY_LEN: usize = ed25519_dalek::SECRET_KEY_LENGTH;

/// The length of an Ed25519 `PublicKey`, in bytes.
pub const PUBLIC_KEY_LEN: usize = ed25519_dalek::PUBLIC_KEY_LENGTH;

/// Private Ed25519 key used for digital signatures.
#[derive(Clone)]
pub struct PrivateKey(ed25519_dalek::SigningKey);

impl Default for PrivateKey {
    fn default() -> Self {
        Self::new()
    }
}

impl PrivateKey {
    /// Generates a new private key using the system's random number generator (CSPRNG) as a seed.
    pub fn new() -> Self {
        let mut csprng: OsRng = OsRng;
        let private_key = ed25519_dalek::SigningKey::generate(&mut csprng);
        Self(private_key)
    }

    /// Create a `PrivateKey` from its raw bytes representation.
    pub fn from_bytes(bytes: &[u8; PRIVATE_KEY_LEN]) -> Self {
        Self(ed25519_dalek::SigningKey::from_bytes(bytes))
    }

    /// Bytes of the private key.
    pub fn as_bytes(&self) -> &[u8; PRIVATE_KEY_LEN] {
        self.0.as_bytes()
    }

    /// Convert the private key to a hex string.
    pub fn to_hex(&self) -> String {
        hex::encode(self.0.as_bytes())
    }

    /// Returns public key using this private counterpart.
    pub fn public_key(&self) -> PublicKey {
        self.0.verifying_key().into()
    }

    /// Sign the provided bytestring using this private key returning a digital signature.
    pub fn sign(&self, bytes: &[u8]) -> Signature {
        self.0.sign(bytes).into()
    }
}

impl fmt::Display for PrivateKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl fmt::Debug for PrivateKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("PrivateKey")
            .field(self.0.as_bytes())
            .finish()
    }
}

impl From<[u8; PRIVATE_KEY_LEN]> for PrivateKey {
    fn from(value: [u8; PRIVATE_KEY_LEN]) -> Self {
        Self::from_bytes(&value)
    }
}

impl From<PrivateKey> for [u8; PRIVATE_KEY_LEN] {
    fn from(value: PrivateKey) -> Self {
        *value.as_bytes()
    }
}

impl From<&[u8; PRIVATE_KEY_LEN]> for PrivateKey {
    fn from(value: &[u8; PRIVATE_KEY_LEN]) -> Self {
        Self::from_bytes(value)
    }
}

impl TryFrom<&[u8]> for PrivateKey {
    type Error = IdentityError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let value_len = value.len();

        let checked_value: [u8; PRIVATE_KEY_LEN] = value
            .try_into()
            .map_err(|_| IdentityError::InvalidLength(value_len, PRIVATE_KEY_LEN))?;

        Ok(Self::from(checked_value))
    }
}

#[cfg(feature = "arbitrary")]
impl<'a> Arbitrary<'a> for PrivateKey {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let bytes = <[u8; PRIVATE_KEY_LEN] as Arbitrary>::arbitrary(u)?;
        Ok(PrivateKey::from_bytes(&bytes))
    }
}

/// Public Ed25519 key used for identifying peers and verifying signed data.
#[derive(Default, Hash, PartialEq, Eq, Copy, Clone)]
pub struct PublicKey(ed25519_dalek::VerifyingKey);

impl PartialOrd for PublicKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PublicKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.to_hex().cmp(&other.to_hex())
    }
}

impl PublicKey {
    /// Create a `PublicKey` from its raw bytes representation.
    pub fn from_bytes(bytes: &[u8; PUBLIC_KEY_LEN]) -> Result<Self, IdentityError> {
        Ok(Self(ed25519_dalek::VerifyingKey::from_bytes(bytes)?))
    }

    /// Bytes of the public key.
    pub fn as_bytes(&self) -> &[u8; PUBLIC_KEY_LEN] {
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

impl fmt::Display for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("PublicKey").field(self.0.as_bytes()).finish()
    }
}

impl From<PublicKey> for ed25519_dalek::VerifyingKey {
    fn from(value: PublicKey) -> Self {
        value.0
    }
}

impl From<ed25519_dalek::VerifyingKey> for PublicKey {
    fn from(value: ed25519_dalek::VerifyingKey) -> Self {
        Self(value)
    }
}

impl TryFrom<[u8; PUBLIC_KEY_LEN]> for PublicKey {
    type Error = IdentityError;

    fn try_from(value: [u8; PUBLIC_KEY_LEN]) -> Result<Self, Self::Error> {
        Self::from_bytes(&value)
    }
}

impl From<PublicKey> for [u8; PUBLIC_KEY_LEN] {
    fn from(value: PublicKey) -> Self {
        *value.as_bytes()
    }
}

impl TryFrom<&[u8; PUBLIC_KEY_LEN]> for PublicKey {
    type Error = IdentityError;

    fn try_from(value: &[u8; PUBLIC_KEY_LEN]) -> Result<Self, Self::Error> {
        Self::from_bytes(value)
    }
}

impl TryFrom<&[u8]> for PublicKey {
    type Error = IdentityError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let value_len = value.len();

        let checked_value: [u8; PUBLIC_KEY_LEN] = value
            .try_into()
            .map_err(|_| IdentityError::InvalidLength(value_len, PUBLIC_KEY_LEN))?;

        Self::try_from(checked_value)
    }
}

impl FromStr for PublicKey {
    type Err = IdentityError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(hex::decode(value)?.as_slice())
    }
}

#[cfg(feature = "arbitrary")]
impl<'a> Arbitrary<'a> for PublicKey {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let bytes = <[u8; PUBLIC_KEY_LEN] as Arbitrary>::arbitrary(u)?;
        let public_key =
            PublicKey::from_bytes(&bytes).map_err(|_| arbitrary::Error::IncorrectFormat)?;
        Ok(public_key)
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
    ///   `PublicKey`.
    ///
    /// * Failure of a signature to satisfy the verification equation.
    #[error("invalid signature: {0}")]
    InvalidSignature(#[from] ed25519_dalek::SignatureError),
}

#[cfg(test)]
mod tests {
    use super::PrivateKey;

    #[test]
    fn signing() {
        let private_key = PrivateKey::new();
        let public_key = private_key.public_key();
        let bytes = b"test";
        let signature = private_key.sign(bytes);
        assert!(public_key.verify(bytes, &signature));

        // Invalid data
        assert!(!public_key.verify(b"not test", &signature));

        // Invalid public key
        let public_key_2 = PrivateKey::new().public_key();
        assert!(!public_key_2.verify(bytes, &signature));
    }
}
