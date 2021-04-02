
use std::convert::TryFrom;

use anyhow::bail;
use ed25519_dalek::{PublicKey, PUBLIC_KEY_LENGTH};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::atomic::Validation;

/// Custom error types for `Author`.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum AuthorError {
    /// Author string does not have the right length.
    #[error("invalid author key length")]
    InvalidLength,

    /// Author string contains invalid hex characters.
    #[error("invalid hex encoding in author string")]
    InvalidHexEncoding,
}

/// Authors are hex encoded ed25519 public key strings.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "db-sqlx", derive(sqlx::Type, sqlx::FromRow), sqlx(transparent))]
pub struct Author(String);

impl Author {
    /// Validates and wraps author string into a new `Author` instance.
    pub fn new(value: &str) -> Result<Self, AuthorError> {
        let author = Self(String::from(value));
        author.validate()?;
        Ok(author)
    }
}

/// Convert Ed25519 `PublicKey` to `Author` instance.
impl TryFrom<PublicKey> for Author {
    type Error = anyhow::Error;

    fn try_from(public_key: PublicKey) -> std::result::Result<Self, Self::Error> {
        Self::new(&hex::encode(public_key.to_bytes()))
    }
}

impl Validation for Author {
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
