// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::{TryInto, TryFrom};
use std::fmt::Display;
use std::hash::Hash as StdHash;
use std::str::FromStr;

use ed25519_dalek_v2::{VerifyingKey, PUBLIC_KEY_LENGTH};
use serde::{Deserialize, Serialize};

use crate::identity::error::PublicKeyError;
use crate::identity::Signature;
use crate::serde::{deserialize_hex, serialize_hex};
use crate::Human;

/// Authors are hex encoded Ed25519 public key strings.
#[derive(Clone, Debug, Copy)]
pub struct PublicKey(VerifyingKey);

impl PublicKey {
    /// Validates and wraps Ed25519 public key string into a new `PublicKey` instance.
    ///
    /// ## Example
    ///
    /// ```
    /// # extern crate p2panda_rs;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// use p2panda_rs::identity::{PublicKey, KeyPair};
    ///
    /// // Generate new Ed25519 key pair
    /// let key_pair = KeyPair::new();
    /// let public_key = key_pair.public_key();
    ///
    /// // Create an `PublicKey` instance from a public key
    /// let public_key = PublicKey::from(public_key);
    ///
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(value: &str) -> Result<Self, PublicKeyError> {
        // Check if hex-encoding is correct
        let bytes = match hex::decode(value) {
            Ok(bytes) => bytes,
            Err(_) => {
                return Err(PublicKeyError::InvalidHexEncoding);
            }
        };

        Self::from_bytes(&bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, PublicKeyError> {
        // Check if length is correct
        let bytes: [u8; PUBLIC_KEY_LENGTH] = bytes
            .try_into()
            .map_err(|_| PublicKeyError::InvalidLength)?;

        let ed25519_public_key = VerifyingKey::from_bytes(&bytes)?;
        Ok(Self(ed25519_public_key))
    }

    /// Returns public_key represented as bytes.
    pub fn to_bytes(&self) -> [u8; PUBLIC_KEY_LENGTH] {
        self.0.to_bytes()
    }

    pub fn verify(&self, bytes: &[u8], signature: &Signature) -> bool {
        self.0.verify_strict(bytes, &signature.into()).is_ok()
    }
}

impl Display for PublicKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(self.to_bytes()))
    }
}

impl Serialize for PublicKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serialize_hex(&self.to_bytes(), serializer)
    }
}

impl<'de> Deserialize<'de> for PublicKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes = deserialize_hex(deserializer)?;
        let public_key = Self::from_bytes(&bytes).map_err(|err| {
            serde::de::Error::custom(format!("invalid public key bytes, {}", err))
        })?;
        Ok(public_key)
    }
}

impl Human for PublicKey {
    /// Return a shortened six character representation.
    ///
    /// ## Example
    ///
    /// ```
    /// # use p2panda_rs::identity::PublicKey;
    /// # use p2panda_rs::Human;
    /// let pub_key = "7cf4f58a2d89e93313f2de99604a814ecea9800cf217b140e9c3a7ba59a5d982";
    /// let public_key = pub_key.parse::<PublicKey>().unwrap();
    /// assert_eq!(public_key.display(), "<PublicKey a5d982>");
    /// ```
    fn display(&self) -> String {
        let offset = PUBLIC_KEY_LENGTH * 2 - 6;
        format!("<PublicKey {}>", &self.to_string()[offset..])
    }
}

impl From<VerifyingKey> for PublicKey {
    fn from(public_key: VerifyingKey) -> Self {
        // Unwrap as we already trust that `PublicKey` is correct
        Self::new(&hex::encode(public_key.to_bytes())).unwrap()
    }
}

impl From<&PublicKey> for VerifyingKey {
    fn from(public_key: &PublicKey) -> Self {
        // Unwrap as we already trust that `PublicKey` is correct
        VerifyingKey::from_bytes(&public_key.to_bytes()).unwrap()
    }
}

impl From<PublicKey> for VerifyingKey {
    fn from(public_key: PublicKey) -> Self {
        // Unwrap as we already trust that `PublicKey` is correct
        VerifyingKey::from_bytes(&public_key.to_bytes()).unwrap()
    }
}

/// Convert any hex-encoded string representation of an Ed25519 public key into an `PublicKey`
/// instance.
impl FromStr for PublicKey {
    type Err = PublicKeyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl TryFrom<String> for PublicKey {
    type Error = PublicKeyError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        PublicKey::from_str(&value)
    }

}

impl StdHash for PublicKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.to_bytes().hash(state)
    }
}

impl PartialEq for PublicKey {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for PublicKey {}

#[cfg(test)]
mod tests {
    use ciborium::cbor;
    use ed25519_dalek_v2::{VerifyingKey, PUBLIC_KEY_LENGTH};
    use serde_bytes::Bytes;

    use crate::identity::error::PublicKeyError;
    use crate::identity::PrivateKey;
    use crate::serde::{deserialize_into, serialize_from, serialize_value};
    use crate::Human;

    use super::PublicKey;

    #[test]
    fn serialize() {
        let public_key =
            PublicKey::new("7cf4f58a2d89e93313f2de99604a814ecea9800cf217b140e9c3a7ba59a5d982")
                .unwrap();
        assert_eq!(
            serialize_from(public_key.clone()),
            serialize_value(cbor!(Bytes::new(&public_key.to_bytes())))
        );
    }

    #[test]
    fn deserialize() {
        let public_key =
            PublicKey::new("7cf4f58a2d89e93313f2de99604a814ecea9800cf217b140e9c3a7ba59a5d982")
                .unwrap();
        assert_eq!(
            deserialize_into::<PublicKey>(&serialize_from(public_key.clone())).unwrap(),
            public_key
        );
    }

    #[test]
    fn validate() {
        // Invalid hexadecimal characters
        assert!(matches!(
            PublicKey::new("vzf4f58a2d89e93313f2de99604a814ezea9800of217b140e9l3a7ba59a5d98p"),
            Err(PublicKeyError::InvalidHexEncoding)
        ));

        // Invalid length
        assert!(matches!(
            PublicKey::new("123456789ffa"),
            Err(PublicKeyError::InvalidLength)
        ));

        // Valid public key string
        assert!(
            PublicKey::new("7cf4f58a2d89e93313f2de99604a814ecea9800cf217b140e9c3a7ba59a5d982")
                .is_ok()
        );
    }

    #[test]
    fn to_bytes() {
        let public_key_bytes: [u8; PUBLIC_KEY_LENGTH] = [
            215, 90, 152, 1, 130, 177, 10, 183, 213, 75, 254, 211, 201, 100, 7, 58, 14, 225, 114,
            243, 218, 166, 35, 37, 175, 2, 26, 104, 247, 7, 81, 26,
        ];

        let public_key = PublicKey::new(&hex::encode(public_key_bytes)).unwrap();
        assert_eq!(public_key.to_bytes(), public_key_bytes);
    }

    #[test]
    fn from_public_key() {
        let public_key_bytes: [u8; PUBLIC_KEY_LENGTH] = [
            215, 90, 152, 1, 130, 177, 10, 183, 213, 75, 254, 211, 201, 100, 7, 58, 14, 225, 114,
            243, 218, 166, 35, 37, 175, 2, 26, 104, 247, 7, 81, 26,
        ];
        let public_key = VerifyingKey::from_bytes(&public_key_bytes).unwrap();

        // Convert `ed25519_dalek` `PublicKey` into `PublicKey` instance
        let public_key: PublicKey = public_key.into();
        assert_eq!(public_key.to_string(), hex::encode(public_key_bytes));
    }

    #[test]
    fn from_str() {
        // Convert string into `PublicKey` instance
        let public_key_str = "7cf4f58a2d89e93313f2de99604a814ecea9800cf217b140e9c3a7ba59a5d982";
        let public_key: PublicKey = public_key_str.parse().unwrap();
        assert_eq!(public_key_str, public_key.to_string());
    }

    #[test]
    fn string_representation() {
        let public_key_str = "7cf4f58a2d89e93313f2de99604a814ecea9800cf217b140e9c3a7ba59a5d982";
        let public_key = PublicKey::new(public_key_str).unwrap();

        assert_eq!(public_key_str, public_key.to_string());
        assert_eq!(public_key_str, public_key.to_string());
        assert_eq!(public_key_str, format!("{}", public_key));
    }

    #[test]
    fn short_representation() {
        let public_key_str = "7cf4f58a2d89e93313f2de99604a814ecea9800cf217b140e9c3a7ba59a5d982";
        let public_key = PublicKey::new(public_key_str).unwrap();

        assert_eq!(public_key.display(), "<PublicKey a5d982>");
    }

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
