use std::convert::TryFrom;

use anyhow::bail;
use arrayvec::ArrayVec;
use bamboo_rs_core::yamf_hash::new_blake2b;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use yamf_hash::{YamfHash, BLAKE2B_HASH_SIZE, MAX_YAMF_HASH_SIZE};

use crate::atomic::Validation;
use crate::Result;

/// This is the type used for `bamboo-rs-core` entries that own their bytes.
pub type Blake2BArrayVec = ArrayVec<[u8; BLAKE2B_HASH_SIZE]>;

/// Custom error types for `Hash`.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum HashError {
    /// Hash string has an invalid length.
    #[error("invalid hash length")]
    InvalidLength,

    /// Hash string contains invalid hex characters.
    #[error("invalid hex encoding in hash string")]
    InvalidHexEncoding,

    /// Hash is not a valid YAMF BLAKE2b hash.
    #[error("can not decode YAMF BLAKE2b hash")]
    DecodingFailed,
}

/// Hash of `Entry` or `Message` encoded as hex string.
///
/// This uses the BLAKE2b algorithm wrapped in [`YAMF`] "Yet-Another-Multi-Format" according to the
/// Bamboo specification.
///
/// [`YAMF`]: https://github.com/bamboo-rs/yamf-hash
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "db-sqlx", derive(sqlx::Type), sqlx(transparent))]
pub struct Hash(String);

impl Hash {
    /// Validates and wraps encoded hash string into new `Hash` instance.
    pub fn new(value: &str) -> Result<Self> {
        let hash = Self(String::from(value));
        hash.validate()?;
        Ok(hash)
    }

    /// Hashes byte data and returns it as `Hash` instance.
    pub fn new_from_bytes(value: Vec<u8>) -> Result<Self> {
        // Generate Blake2b hash
        let blake2b_hash = new_blake2b(&value);

        // Wrap hash in YAMF container format
        let mut bytes = Vec::new();
        blake2b_hash.encode_write(&mut bytes)?;

        // Encode bytes as hex string
        let hex_str = hex::encode(&bytes);

        Ok(Self(hex_str))
    }

    /// Returns hash as bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        // Unwrap as we already validated the hash
        hex::decode(&self.0).unwrap()
    }

    /// Returns hash as hex string.
    pub fn as_hex(&self) -> &str {
        self.0.as_str()
    }
}

/// Converts YAMF hash from `yamf-hash` crate to p2panda `Hash` instance.
impl<T: core::borrow::Borrow<[u8]>> TryFrom<YamfHash<T>> for Hash {
    type Error = anyhow::Error;

    fn try_from(yamf_hash: YamfHash<T>) -> std::result::Result<Self, Self::Error> {
        let mut out = [0u8; MAX_YAMF_HASH_SIZE];
        let _ = yamf_hash.encode(&mut out)?;
        Self::new(&hex::encode(out))
    }
}

/// Returns Yet-Another-Multiformat Hash struct from the `yamf-hash` crate.
///
/// This comes in handy when interacting with the `bamboo-rs` crate.
impl From<Hash> for YamfHash<Blake2BArrayVec> {
    fn from(hash: Hash) -> YamfHash<Blake2BArrayVec> {
        let bytes = hash.to_bytes();
        let yamf_hash = YamfHash::<Blake2BArrayVec>::decode_owned(&bytes).unwrap();
        yamf_hash.0
    }
}

impl Validation for Hash {
    fn validate(&self) -> Result<()> {
        // Check if hash is a hex string
        match hex::decode(self.0.to_owned()) {
            Ok(bytes) => {
                // Check if length is correct
                if bytes.len() != BLAKE2B_HASH_SIZE + 2 {
                    bail!(HashError::InvalidLength)
                }

                // Check if YAMF BLAKE2b hash is valid
                match YamfHash::<&[u8]>::decode(&bytes) {
                    Ok((YamfHash::Blake2b(_), _)) => {}
                    _ => bail!(HashError::DecodingFailed),
                }
            }
            Err(_) => bail!(HashError::InvalidHexEncoding),
        }

        Ok(())
    }
}

impl PartialEq for Hash {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryInto;

    use yamf_hash::YamfHash;

    use super::{Blake2BArrayVec, Hash};

    #[test]
    fn validate() {
        assert!(Hash::new("abcdefg").is_err());
        assert!(Hash::new("112233445566ff").is_err());
        assert!(
            Hash::new("01234567812345678123456781234567812345678123456781234567812345678").is_err()
        );
        assert!(
            Hash::new("004069db5208a271c53de8a1b6220e6a4d7fcccd89e6c0c7e75c833e34dc68d932624f2ccf27513f42fb7d0e4390a99b225bad41ba14a6297537246dbe4e6ce150e8").is_ok()
        );
    }

    #[test]
    fn new_from_bytes() {
        assert_eq!(Hash::new_from_bytes(vec![1, 2, 3]).unwrap(), Hash::new("0040cf94f6d605657e90c543b0c919070cdaaf7209c5e1ea58acb8f3568fa2114268dc9ac3bafe12af277d286fce7dc59b7c0c348973c4e9dacbe79485e56ac2a702").unwrap());
    }

    #[test]
    fn convert_yamf_hash() {
        let hash = Hash::new_from_bytes(vec![1, 2, 3]).unwrap();
        let yamf_hash = Into::<YamfHash<Blake2BArrayVec>>::into(hash.to_owned());
        let hash_restored = TryInto::<Hash>::try_into(yamf_hash).unwrap();
        assert_eq!(hash, hash_restored);
    }
}
