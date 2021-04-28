use std::convert::{TryFrom, TryInto};

use arrayvec::ArrayVec;
use bamboo_rs_core::Entry;
use ed25519_dalek::Signature;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::atomic::{Author, Hash, Validation};

/// Custom error types for `EntrySigned`.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum EntrySignedError {
    /// Encoded entry string contains invalid hex characters.
    #[error("invalid hex encoding in entry")]
    InvalidHexEncoding,

    /// Message needs to match payload hash of encoded entry
    #[error("message needs to match payload hash of encoded entry")]
    MessageHashMismatch,
 
    /// Can not sign and encode an entry without a `Message`.
    #[error("entry does not contain any message")]
    MessageMissing,

    /// Skiplink is required for entry encoding.
    #[error("entry requires skiplink for encoding")]
    SkiplinkMissing,
       
    /// Handle errors from [`atomic::SeqNum`] struct.
    #[error(transparent)]
    SeqNumError(#[from] crate::atomic::error::SeqNumError),
        
    /// Handle errors from [`atomic::Hash`] struct.
    #[error(transparent)]
    HashError(#[from] crate::atomic::error::HashError),
   
    /// Handle errors from [`atomic::MessageEncoded`] struct.
    #[error(transparent)]
    MessageEncodedError(#[from] crate::atomic::error::MessageEncodedError),

    /// Handle errors from encoding bamboo_rs_core entries.
    #[error(transparent)]
    BambooEncodeError(#[from] bamboo_rs_core::entry::encode::Error),

    /// Handle errors from ed25519_dalek crate.
    #[error(transparent)]
    Ed25519SignatureError(#[from] ed25519_dalek::SignatureError),
}

/// Bamboo entry bytes represented in hex encoding format.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(
    feature = "db-sqlx",
    derive(sqlx::Type, sqlx::FromRow),
    sqlx(transparent)
)]
pub struct EntrySigned(String);

impl EntrySigned {
    /// Validates and wraps encoded entry string into a new `EntrySigned` instance.
    pub fn new(value: &str) -> Result<Self, EntrySignedError> {
        let inner = Self(value.to_owned());
        inner.validate()?;
        Ok(inner)
    }

    /// Returns YAMF BLAKE2b hash of encoded entry.
    pub fn hash(&self) -> Hash {
        Hash::new_from_bytes(self.to_bytes()).unwrap()
    }

    /// Returns `Author` who signed this entry.
    pub fn author(&self) -> Author {
        // Unwrap as we already validated entry
        let entry: Entry<ArrayVec<[u8; 64]>, ArrayVec<[u8; 64]>> = self.try_into().unwrap();
        Author::try_from(entry.author).unwrap()
    }

    /// Returns Ed25519 signature of this entry.
    pub fn signature(&self) -> Signature {
        // Unwrap as we already validated entry and know it contains a signature
        let entry: Entry<ArrayVec<[u8; 64]>, ArrayVec<[u8; 64]>> = self.try_into().unwrap();

        // Convert into Ed25519 Signature instance
        let array_vec = entry.sig.unwrap().0;
        Signature::new(array_vec.into_inner().unwrap())
    }

    /// Returns encoded entry as string.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Decodes hex encoding and returns entry as bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        // Unwrap as we already know that the inner value is valid
        hex::decode(&self.0).unwrap()
    }

    /// Returns payload size (number of bytes) of total encoded entry.
    pub fn size(&self) -> i64 {
        self.0.len() as i64 / 2
    }    
}

/// Converts an `EntrySigned` into a Bamboo Entry to interact with the `bamboo_rs` crate.
impl From<&EntrySigned> for Entry<ArrayVec<[u8; 64]>, ArrayVec<[u8; 64]>> {
    fn from(signed_entry: &EntrySigned) -> Self {
        let entry_bytes = signed_entry.clone().to_bytes();
        let entry_ref: Entry<&[u8], &[u8]> = entry_bytes.as_slice().try_into().unwrap();
        bamboo_rs_core::entry::into_owned(&entry_ref)
    }
}

impl TryFrom<&[u8]> for EntrySigned {
    type Error = EntrySignedError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        Self::new(&hex::encode(bytes))
    }
}

impl PartialEq for EntrySigned {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Validation for EntrySigned {
    type Error = EntrySignedError;

    fn validate(&self) -> Result<(), Self::Error> {
        hex::decode(&self.0).map_err(|_| EntrySignedError::InvalidHexEncoding)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::EntrySigned;

    #[test]
    fn validate() {
        // Invalid hex string
        assert!(EntrySigned::new("123456789Z").is_err());
    }
}
