// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;
use std::fmt::Display;
use std::hash::Hash as StdHash;
use std::str::FromStr;

use ed25519_dalek::{PublicKey, PUBLIC_KEY_LENGTH};
use serde::{Deserialize, Serialize};

use crate::identity::AuthorError;
use crate::Validate;

/// Authors are hex encoded Ed25519 public key strings.
#[derive(Clone, Debug, Serialize, Eq, StdHash, Deserialize, PartialEq)]
pub struct Author(String);

impl Author {
    /// Validates and wraps Ed25519 public key string into a new `Author` instance.
    ///
    /// ## Example
    ///
    /// ```
    /// # extern crate p2panda_rs;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// use std::convert::TryFrom;
    ///
    /// use p2panda_rs::identity::{Author, KeyPair};
    ///
    /// // Generate new Ed25519 key pair
    /// let key_pair = KeyPair::new();
    /// let public_key = key_pair.public_key().to_owned();
    ///
    /// // Create an `Author` instance from a public key
    /// let author = Author::try_from(public_key).unwrap();
    ///
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(value: &str) -> Result<Self, AuthorError> {
        let author = Self(String::from(value));
        author.validate()?;
        Ok(author)
    }

    /// Returns hexadecimal representation of public key bytes as `&str`.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Returns hexadecimal representation of public key bytes as `String`.
    pub fn to_string(&self) -> String {
        self.0.clone()
    }

    /// Return a shortened six character representation.
    ///
    /// ## Example
    ///
    /// ```
    /// # use p2panda_rs::identity::Author;
    /// let pub_key = "7cf4f58a2d89e93313f2de99604a814ecea9800cf217b140e9c3a7ba59a5d982";
    /// let author = pub_key.parse::<Author>().unwrap();
    /// assert_eq!(author.as_short_str(), "a5d982");
    /// ```
    pub fn as_short_str(&self) -> &str {
        // Display last 6 of 64 hexadecimal characters of public key string
        &self.0[58..]
    }
}

impl Display for Author {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<Author {}>", self.as_short_str())
    }
}

/// Convert ed25519_dalek `PublicKey` to `Author` instance.
impl TryFrom<PublicKey> for Author {
    type Error = AuthorError;

    fn try_from(public_key: PublicKey) -> Result<Self, Self::Error> {
        Self::new(&hex::encode(public_key.to_bytes()))
    }
}

/// Convert any hex-encoded string representation of an Ed25519 public key into an `Author`
/// instance.
impl FromStr for Author {
    type Err = AuthorError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl Validate for Author {
    type Error = AuthorError;

    fn validate(&self) -> Result<(), Self::Error> {
        // Check if author is hex encoded
        match hex::decode(&self.0) {
            Ok(bytes) => {
                // Check if length is correct
                if bytes.len() != PUBLIC_KEY_LENGTH {
                    return Err(AuthorError::InvalidLength);
                }
            }
            Err(_) => {
                return Err(AuthorError::InvalidHexEncoding);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryInto;

    use ed25519_dalek::{PublicKey, PUBLIC_KEY_LENGTH};

    use crate::identity::AuthorError;

    use super::Author;

    #[test]
    fn validate() {
        // Invalid hexadecimal characters
        assert!(matches!(
            Author::new("vzf4f58a2d89e93313f2de99604a814ezea9800of217b140e9l3a7ba59a5d98p"),
            Err(AuthorError::InvalidHexEncoding)
        ));

        // Invalid length
        assert!(matches!(
            Author::new("123456789ffa"),
            Err(AuthorError::InvalidLength)
        ));

        // Valid public key string
        assert!(
            Author::new("7cf4f58a2d89e93313f2de99604a814ecea9800cf217b140e9c3a7ba59a5d982").is_ok()
        );
    }

    #[test]
    fn from_public_key() {
        let public_key_bytes: [u8; PUBLIC_KEY_LENGTH] = [
            215, 90, 152, 1, 130, 177, 10, 183, 213, 75, 254, 211, 201, 100, 7, 58, 14, 225, 114,
            243, 218, 166, 35, 37, 175, 2, 26, 104, 247, 7, 81, 26,
        ];
        let public_key = PublicKey::from_bytes(&public_key_bytes).unwrap();

        // Convert `ed25519_dalek` `PublicKey` into `Author` instance
        let author: Author = public_key.try_into().unwrap();
        assert_eq!(author.to_string(), hex::encode(public_key_bytes));
    }

    #[test]
    fn from_str() {
        // Convert string into `Author` instance
        let author_str = "7cf4f58a2d89e93313f2de99604a814ecea9800cf217b140e9c3a7ba59a5d982";
        let author: Author = author_str.parse().unwrap();
        assert_eq!(author_str, author.as_str());
    }

    #[test]
    fn string_representation() {
        let author_str = "7cf4f58a2d89e93313f2de99604a814ecea9800cf217b140e9c3a7ba59a5d982";
        let author = Author::new(author_str).unwrap();

        // Long string representation via `Debug` trait and functions
        assert_eq!(author_str, author.as_str());
        assert_eq!(author_str, author.to_string());
        assert_ne!(format!("{:?}", author), author.as_short_str());

        // Short string representation via `Display` trait and function
        assert_eq!(format!("{}", author), "<Author a5d982>");
        assert_eq!(author.as_short_str(), "a5d982");
    }
}
