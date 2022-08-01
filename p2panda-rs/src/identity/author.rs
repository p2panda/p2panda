// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Display;
use std::hash::Hash as StdHash;
use std::str::FromStr;

use ed25519_dalek::{PublicKey, PUBLIC_KEY_LENGTH};
use serde::{Deserialize, Serialize};

use crate::identity::AuthorError;
use crate::{Human, Validate};

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
    /// use p2panda_rs::identity::{Author, KeyPair};
    ///
    /// // Generate new Ed25519 key pair
    /// let key_pair = KeyPair::new();
    /// let public_key = key_pair.public_key();
    ///
    /// // Create an `Author` instance from a public key
    /// let author = Author::from(public_key);
    ///
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(value: &str) -> Result<Self, AuthorError> {
        let author = Self(String::from(value));
        author.validate()?;
        Ok(author)
    }

    /// Return bytes of author.
    pub fn to_bytes(&self) -> Vec<u8> {
        // Unwrap as we already checked the inner hex values
        hex::decode(&self.0).unwrap()
    }

    /// Returns hexadecimal representation of public key bytes as `&str`.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl Display for Author {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Human for Author {
    /// Return a shortened six character representation.
    ///
    /// ## Example
    ///
    /// ```
    /// # use p2panda_rs::identity::Author;
    /// # use p2panda_rs::Human;
    /// let pub_key = "7cf4f58a2d89e93313f2de99604a814ecea9800cf217b140e9c3a7ba59a5d982";
    /// let author = pub_key.parse::<Author>().unwrap();
    /// assert_eq!(author.display(), "<Author a5d982>");
    /// ```
    fn display(&self) -> String {
        let offset = PUBLIC_KEY_LENGTH * 2 - 6;
        format!("<Author {}>", &self.0[offset..])
    }
}

/// Convert ed25519_dalek `PublicKey` to `Author` instance.
impl From<&PublicKey> for Author {
    fn from(public_key: &PublicKey) -> Self {
        // Unwrap as we already trust that `PublicKey` is correct
        Self::new(&hex::encode(public_key.to_bytes())).unwrap()
    }
}

impl From<&Author> for PublicKey {
    fn from(author: &Author) -> Self {
        // Unwrap as we already trust that `Author` is correct
        PublicKey::from_bytes(&author.to_bytes()).unwrap()
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
    use ed25519_dalek::{PublicKey, PUBLIC_KEY_LENGTH};

    use crate::identity::AuthorError;
    use crate::Human;

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
    fn to_bytes() {
        let public_key_bytes: [u8; PUBLIC_KEY_LENGTH] = [
            215, 90, 152, 1, 130, 177, 10, 183, 213, 75, 254, 211, 201, 100, 7, 58, 14, 225, 114,
            243, 218, 166, 35, 37, 175, 2, 26, 104, 247, 7, 81, 26,
        ];

        let author = Author::new(&hex::encode(public_key_bytes)).unwrap();
        assert_eq!(author.to_bytes(), public_key_bytes.to_vec());
    }

    #[test]
    fn from_public_key() {
        let public_key_bytes: [u8; PUBLIC_KEY_LENGTH] = [
            215, 90, 152, 1, 130, 177, 10, 183, 213, 75, 254, 211, 201, 100, 7, 58, 14, 225, 114,
            243, 218, 166, 35, 37, 175, 2, 26, 104, 247, 7, 81, 26,
        ];
        let public_key = PublicKey::from_bytes(&public_key_bytes).unwrap();

        // Convert `ed25519_dalek` `PublicKey` into `Author` instance
        let author: Author = (&public_key).into();
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

        assert_eq!(author_str, author.as_str());
        assert_eq!(author_str, author.to_string());
        assert_eq!(author_str, format!("{}", author));
    }

    #[test]
    fn short_representation() {
        let author_str = "7cf4f58a2d89e93313f2de99604a814ecea9800cf217b140e9c3a7ba59a5d982";
        let author = Author::new(author_str).unwrap();

        assert_eq!(author.display(), "<Author a5d982>");
    }
}
