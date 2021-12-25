// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;

use ed25519_dalek::{PublicKey, PUBLIC_KEY_LENGTH};
use serde::{Deserialize, Serialize};

use crate::identity::AuthorError;
use crate::Validate;

/// Authors are hex encoded Ed25519 public key strings.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(
    feature = "db-sqlx",
    derive(sqlx::Type, sqlx::FromRow),
    sqlx(transparent)
)]
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
    /// use p2panda_rs::identity::{KeyPair, Author};
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

    /// Returns author as hex string.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Convert Ed25519 `PublicKey` to `Author` instance.
impl TryFrom<PublicKey> for Author {
    type Error = AuthorError;

    fn try_from(public_key: PublicKey) -> Result<Self, Self::Error> {
        Self::new(&hex::encode(public_key.to_bytes()))
    }
}

impl Validate for Author {
    type Error = AuthorError;

    fn validate(&self) -> Result<(), Self::Error> {
        // Check if author is hex encoded
        match hex::decode(self.0.to_owned()) {
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
    use super::Author;

    #[test]
    fn validate() {
        assert!(Author::new("abcdefg").is_err());
        assert!(Author::new("112233445566ff").is_err());
        assert!(
            Author::new("7cf4f58a2d89e93313f2de99604a814ecea9800cf217b140e9c3a7ba59a5d982").is_ok()
        );
    }
}
