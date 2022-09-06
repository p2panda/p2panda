// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Display;
use std::hash::Hash as StdHash;
use std::str::FromStr;

use ed25519_dalek::{PublicKey as Ed25519PublicKey, PUBLIC_KEY_LENGTH};
use serde::{Deserialize, Deserializer, Serialize};

use crate::identity::error::PublicKeyError;
use crate::{Human, Validate};

/// Authors are hex encoded Ed25519 public key strings.
#[derive(Clone, Debug, Serialize, Copy)]
pub struct PublicKey(Ed25519PublicKey);

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
        let bytes = match hex::decode(value) {
            Ok(bytes) => {
                // Check if length is correct
                if bytes.len() != PUBLIC_KEY_LENGTH {
                    return Err(PublicKeyError::InvalidLength);
                }
                bytes
            }
            Err(_) => {
                return Err(PublicKeyError::InvalidHexEncoding);
            }
        };

        let ed25519_public_key = Ed25519PublicKey::from_bytes(&bytes)?;
        let public_key = Self(ed25519_public_key);
        public_key.validate()?;
        Ok(public_key)
    }

    /// Returns public_key represented as bytes.
    pub fn to_bytes(&self) -> [u8; PUBLIC_KEY_LENGTH] {
        self.0.to_bytes()
    }
}

impl Display for PublicKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(self.to_bytes()))
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

impl<'de> Deserialize<'de> for PublicKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Deserialize into public key string
        let public_key: String = Deserialize::deserialize(deserializer)?;

        // Check format
        PublicKey::new(&public_key)
            .map_err(|err| serde::de::Error::custom(format!("invalid public key {}", err)))
    }
}

/// Convert ed25519_dalek `PublicKey` to `PublicKey` instance.
impl From<&Ed25519PublicKey> for PublicKey {
    fn from(public_key: &Ed25519PublicKey) -> Self {
        // Unwrap as we already trust that `PublicKey` is correct
        Self::new(&hex::encode(public_key.to_bytes())).unwrap()
    }
}

impl From<&PublicKey> for Ed25519PublicKey {
    fn from(public_key: &PublicKey) -> Self {
        // Unwrap as we already trust that `PublicKey` is correct
        Ed25519PublicKey::from_bytes(&public_key.to_bytes()).unwrap()
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

impl Validate for PublicKey {
    type Error = PublicKeyError;

    fn validate(&self) -> Result<(), Self::Error> {
        // Check if public_key is hex encoded
        match hex::decode(&self.0) {
            Ok(bytes) => {
                // Check if length is correct
                if bytes.len() != PUBLIC_KEY_LENGTH {
                    return Err(PublicKeyError::InvalidLength);
                }
            }
            Err(_) => {
                return Err(PublicKeyError::InvalidHexEncoding);
            }
        }

        Ok(())
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
    use ed25519_dalek::{PublicKey as Ed25519PublicKey, PUBLIC_KEY_LENGTH};

    use crate::identity::error::PublicKeyError;
    use crate::Human;

    use super::PublicKey;

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
        let public_key = Ed25519PublicKey::from_bytes(&public_key_bytes).unwrap();

        // Convert `ed25519_dalek` `PublicKey` into `PublicKey` instance
        let public_key: PublicKey = (&public_key).into();
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
}
