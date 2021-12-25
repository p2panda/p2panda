// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;

use arrayvec::ArrayVec;
use bamboo_rs_core_ed25519_yasmf::yasmf_hash::new_blake3;
use serde::{Deserialize, Serialize};
use yasmf_hash::{YasmfHash, BLAKE3_HASH_SIZE, MAX_YAMF_HASH_SIZE};

use crate::hash::HashError;
use crate::Validate;

/// Size of p2panda entries' hashes.
pub(crate) const HASH_SIZE: usize = BLAKE3_HASH_SIZE;

/// Type used for `bamboo-rs-core-ed25519-yasmf` entries that own their bytes.
pub type Blake3ArrayVec = ArrayVec<[u8; HASH_SIZE]>;

/// Hash of `Entry` or `Operation` encoded as hex string.
///
/// This uses the BLAKE3 algorithm wrapped in [`YASMF`] "Yet-Another-Smol-Multi-Format" according
/// to the Bamboo specification.
///
/// [`YASMF`]: https://github.com/bamboo-rs/yasmf-hash
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(
    feature = "db-sqlx",
    derive(sqlx::Type, sqlx::FromRow),
    sqlx(transparent)
)]
pub struct Hash(String);

impl Hash {
    /// Validates and wraps encoded hash string into new `Hash` instance.
    pub fn new(value: &str) -> Result<Self, HashError> {
        let hash = Self(String::from(value));
        hash.validate()?;
        Ok(hash)
    }

    /// Hashes byte data and returns it as `Hash` instance.
    pub fn new_from_bytes(value: Vec<u8>) -> Result<Self, HashError> {
        // Generate Blake3 hash
        let blake3_hash = new_blake3(&value);

        // Wrap hash in YASMF container format
        let mut bytes = Vec::new();
        blake3_hash.encode_write(&mut bytes)?;

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
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Converts YASMF hash from `yasmf-hash` crate to p2panda `Hash` instance.
impl<T: core::borrow::Borrow<[u8]>> TryFrom<YasmfHash<T>> for Hash {
    type Error = HashError;

    fn try_from(yasmf_hash: YasmfHash<T>) -> Result<Self, Self::Error> {
        let mut out = [0u8; MAX_YAMF_HASH_SIZE];
        let _ = yasmf_hash.encode(&mut out)?;
        Self::new(&hex::encode(out))
    }
}

/// Returns Yet-Another-Smol-Multiformat Hash struct from the `yasmf-hash` crate.
///
/// This comes in handy when interacting with the `bamboo-rs` crate.
impl From<Hash> for YasmfHash<Blake3ArrayVec> {
    fn from(hash: Hash) -> YasmfHash<Blake3ArrayVec> {
        let bytes = hash.to_bytes();
        let yasmf_hash = YasmfHash::<Blake3ArrayVec>::decode_owned(&bytes).unwrap();
        yasmf_hash.0
    }
}

impl Validate for Hash {
    type Error = HashError;

    fn validate(&self) -> Result<(), Self::Error> {
        // Check if hash is a hex string
        match hex::decode(self.0.to_owned()) {
            Ok(bytes) => {
                // Check if length is correct
                if bytes.len() != HASH_SIZE + 2 {
                    return Err(HashError::InvalidLength(bytes.len(), HASH_SIZE + 2));
                }

                // Check if YASMF BLAKE3 hash is valid
                match YasmfHash::<&[u8]>::decode(&bytes) {
                    Ok((YasmfHash::Blake3(_), _)) => {}
                    _ => return Err(HashError::DecodingFailed),
                }
            }
            Err(_) => return Err(HashError::InvalidHexEncoding),
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

    use yasmf_hash::YasmfHash;

    use super::{Blake3ArrayVec, Hash};

    #[test]
    fn validate() {
        assert!(Hash::new("abcdefg").is_err());
        assert!(Hash::new("112233445566ff").is_err());
        assert!(
            Hash::new("01234567812345678123456781234567812345678123456781234567812345678").is_err()
        );
        assert!(
            Hash::new("0020b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543")
                .is_ok()
        );
    }

    #[test]
    fn new_from_bytes() {
        assert_eq!(
            Hash::new_from_bytes(vec![1, 2, 3]).unwrap(),
            Hash::new("0020b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543")
                .unwrap()
        );
    }

    #[test]
    fn convert_yamf_hash() {
        let hash = Hash::new_from_bytes(vec![1, 2, 3]).unwrap();
        let yamf_hash = Into::<YasmfHash<Blake3ArrayVec>>::into(hash.to_owned());
        let hash_restored = TryInto::<Hash>::try_into(yamf_hash).unwrap();
        assert_eq!(hash, hash_restored);
    }
}
