use ed25519_dalek::PUBLIC_KEY_LENGTH;
use validator::{Validate, ValidationError, ValidationErrors};

use crate::error::{Result, ValidationResult};

/// Authors are hex encoded ed25519 public key strings.
#[derive(Clone, Debug)]
pub struct Author(String);

impl Author {
    /// Validates and returns an author when correct.
    #[allow(dead_code)]
    pub fn new(value: &str) -> Result<Self> {
        let author = Self(String::from(value));
        author.validate()?;
        Ok(author)
    }
}

impl Validate for Author {
    fn validate(&self) -> ValidationResult {
        let mut errors = ValidationErrors::new();

        // Check if author is hex encoded
        match hex::decode(self.0.to_owned()) {
            Ok(bytes) => {
                // Check if length is correct
                if bytes.len() != PUBLIC_KEY_LENGTH {
                    errors.add("author", ValidationError::new("invalid string length"));
                }
            }
            Err(_) => {
                errors.add("author", ValidationError::new("invalid hex string"));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
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
