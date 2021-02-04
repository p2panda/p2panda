use thiserror::Error;

use crate::Result;
use crate::atomic::{Entry, Hash, Validation};

/// Custom error types for `EntryEncoded`
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum EntryEncodedError {
    /// Encoded message string contains invalid hex characters.
    #[error("invalid hex encoding in message")]
    InvalidHexEncoding,
}

/// Bamboo entry bytes represented in hex encoding format.
#[derive(Clone, Debug)]
pub struct EntryEncoded(String);

impl EntryEncoded {
    /// Validates and returns a new encoded entry instance.
    pub fn new(value: &str) -> Result<Self> {
        let inner = Self(value.to_owned());
        inner.validate()?;
        Ok(inner)
    }

    /// Returns decoded version of this entry.
    pub fn decode(&self) -> Entry {
        todo!();
    }

    /// Returns YAMF BLAKE2b hash of encoded entry.
    pub fn hash(&self) -> Hash {
        Hash::from_bytes(self.as_bytes()).unwrap()
    }

    /// Returns encoded entry as string.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Decodes hex encoding and returns entry as bytes.
    pub fn as_bytes(&self) -> Vec<u8> {
        // Unwrap as we already know that the inner value is valid
        hex::decode(&self.0).unwrap()
    }

    /// Returns payload size (number of bytes) of total encoded entry.
    pub fn size(&self) -> u64 {
        self.0.len() as u64 / 2
    }
}

impl Validation for EntryEncoded {
    fn validate(&self) -> Result<()> {
        hex::decode(&self.0).map_err(|_| EntryEncodedError::InvalidHexEncoding)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::EntryEncoded;

    #[test]
    fn validate() {
        // Invalid hex string
        assert!(EntryEncoded::new("123456789Z").is_err());
    }
}
