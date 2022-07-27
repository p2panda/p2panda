// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;
use std::fmt;
use std::hash::Hash as StdHash;
use std::str::FromStr;

use arrayvec::ArrayVec;
use bamboo_rs_core_ed25519_yasmf::yasmf_hash::new_blake3;
use serde::{Deserialize, Serialize};
use yasmf_hash::{YasmfHash, BLAKE3_HASH_SIZE, MAX_YAMF_HASH_SIZE};

use crate::hash::HashError;
use crate::{Human, Validate};

/// Size of p2panda entries' hashes.
pub const HASH_SIZE: usize = BLAKE3_HASH_SIZE;

/// Type used for `bamboo-rs-core-ed25519-yasmf` entries that own their bytes.
pub type Blake3ArrayVec = ArrayVec<[u8; HASH_SIZE]>;

/// Hash of `Entry` or `Operation` encoded as hex string.
///
/// This uses the BLAKE3 algorithm wrapped in [`YASMF`] "Yet-Another-Smol-Multi-Format" according
/// to the Bamboo specification.
///
/// [`YASMF`]: https://github.com/bamboo-rs/yasmf-hash
#[derive(Clone, Debug, Ord, PartialOrd, Serialize, Deserialize, PartialEq, Eq, StdHash)]
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

    /// Returns hash as `&str`.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Human for Hash {
    /// Return a shortened six character representation.
    ///
    /// ## Example
    ///
    /// ```
    /// # use p2panda_rs::hash::Hash;
    /// # use p2panda_rs::Human;
    /// let hash_str = "0020cfb0fa37f36d082faad3886a9ffbcc2813b7afe90f0609a556d425f1a76ec805";
    /// let hash: Hash = hash_str.parse().unwrap();
    /// assert_eq!(hash.display(), "<Hash 6ec805>");
    /// ```
    fn display(&self) -> String {
        let offset = MAX_YAMF_HASH_SIZE * 2 - 6;
        format!("<Hash {}>", &self.as_str()[offset..])
    }
}

/// Converts YASMF hash from `yasmf-hash` crate to p2panda `Hash` instance.
impl<T: core::borrow::Borrow<[u8]> + Clone> TryFrom<YasmfHash<T>> for Hash {
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

/// Convert any hex-encoded string representation of a hash into a `Hash` instance.
impl TryFrom<&str> for Hash {
    type Error = HashError;

    fn try_from(str: &str) -> Result<Self, Self::Error> {
        Self::new(str)
    }
}

/// Convert any borrowed string representation into a `Hash` instance.
impl FromStr for Hash {
    type Err = HashError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

/// Convert any owned string representation into a `Hash` instance.
impl TryFrom<String> for Hash {
    type Error = HashError;

    fn try_from(str: String) -> Result<Self, Self::Error> {
        Self::new(&str)
    }
}

impl Validate for Hash {
    type Error = HashError;

    fn validate(&self) -> Result<(), Self::Error> {
        // Check if hash is a hex string
        match hex::decode(&self.0) {
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::convert::{TryFrom, TryInto};

    use yasmf_hash::YasmfHash;

    use crate::Human;

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
    fn convert_yasmf() {
        let hash = Hash::new_from_bytes(vec![1, 2, 3]).unwrap();
        let yasmf_hash = Into::<YasmfHash<Blake3ArrayVec>>::into(hash.to_owned());
        let hash_restored = TryInto::<Hash>::try_into(yasmf_hash).unwrap();
        assert_eq!(hash, hash_restored);
    }

    #[test]
    fn it_hashes() {
        let hash = Hash::new_from_bytes(vec![1, 2, 3]).unwrap();
        let mut hash_map = HashMap::new();
        let key_value = "Value identified by a hash".to_string();
        hash_map.insert(&hash, key_value.clone());
        let key_value_retrieved = hash_map.get(&hash).unwrap().to_owned();
        assert_eq!(key_value, key_value_retrieved)
    }

    #[test]
    fn from_string() {
        let hash_str = "0020b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543";

        // Using TryFrom<&str>
        let hash_from_str: Hash = hash_str.try_into().unwrap();
        assert_eq!(hash_str, hash_from_str.as_str());

        // Using FromStr
        let hash_from_parse: Hash = hash_str.parse().unwrap();
        assert_eq!(hash_str, hash_from_parse.as_str());

        // Using TryFrom<String>
        let hash_from_string = Hash::try_from(String::from(hash_str)).unwrap();
        assert_eq!(hash_str, hash_from_string.as_str());
    }

    #[test]
    fn string_representation() {
        let hash_str = "0020b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543";
        let hash = Hash::new(hash_str).unwrap();

        assert_eq!(hash_str, hash.as_str());
        assert_eq!(hash_str, hash.to_string());
        assert_eq!(hash_str, format!("{}", hash));
    }

    #[test]
    fn short_representation() {
        let hash_str = "0020b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543";
        let hash = Hash::new(hash_str).unwrap();

        assert_eq!(hash.display(), "<Hash 496543>");
    }
}
