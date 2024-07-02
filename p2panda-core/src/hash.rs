// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::serde::{deserialize_hex, serialize_hex};

/// Size of BLAKE3 hashes.
pub const HASH_LEN: usize = blake3::KEY_LEN;

/// 32-byte BLAKE3 hash.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Hash(blake3::Hash);

impl Hash {
    /// Calculate the hash of the provided bytes.
    pub fn new(buf: impl AsRef<[u8]>) -> Self {
        Self(blake3::hash(buf.as_ref()))
    }

    /// Create a `Hash` from its raw bytes representation.
    pub const fn from_bytes(bytes: [u8; HASH_LEN]) -> Self {
        Self(blake3::Hash::from_bytes(bytes))
    }

    /// Bytes of the hash.
    pub fn as_bytes(&self) -> &[u8; HASH_LEN] {
        self.0.as_bytes()
    }

    /// Convert the hash to a hex string.
    pub fn to_hex(&self) -> String {
        self.0.to_hex().to_string()
    }
}

impl AsRef<[u8]> for Hash {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl From<Hash> for blake3::Hash {
    fn from(value: Hash) -> Self {
        value.0
    }
}

impl From<blake3::Hash> for Hash {
    fn from(value: blake3::Hash) -> Self {
        Self(value)
    }
}

impl From<[u8; HASH_LEN]> for Hash {
    fn from(value: [u8; HASH_LEN]) -> Self {
        Self(blake3::Hash::from(value))
    }
}

impl From<Hash> for [u8; HASH_LEN] {
    fn from(value: Hash) -> Self {
        *value.as_bytes()
    }
}

impl From<&[u8; HASH_LEN]> for Hash {
    fn from(value: &[u8; HASH_LEN]) -> Self {
        Self(blake3::Hash::from(*value))
    }
}

impl TryFrom<&[u8]> for Hash {
    type Error = HashError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let value_len = value.len();

        let checked_value: [u8; HASH_LEN] = value
            .try_into()
            .map_err(|_| HashError::InvalidLength(value_len, HASH_LEN))?;

        Ok(Self(blake3::Hash::from(checked_value)))
    }
}

impl FromStr for Hash {
    type Err = HashError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(hex::decode(value)?.as_slice())
    }
}

impl PartialOrd for Hash {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.0.as_bytes().cmp(other.0.as_bytes()))
    }
}

impl Ord for Hash {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.as_bytes().cmp(other.0.as_bytes())
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl fmt::Debug for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Hash").field(self.0.as_bytes()).finish()
    }
}

impl Serialize for Hash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serialize_hex(self.0.as_bytes(), serializer)
    }
}

impl<'de> Deserialize<'de> for Hash {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes = deserialize_hex(deserializer)?;

        bytes
            .as_slice()
            .try_into()
            .map_err(|err: HashError| serde::de::Error::custom(err.to_string()))
    }
}

/// Error types for `Hash` struct.
#[derive(Error, Debug)]
pub enum HashError {
    /// Hash string has an invalid length.
    #[error("invalid hash length {0} bytes, expected {1} bytes")]
    InvalidLength(usize, usize),

    /// Hash string contains invalid hexadecimal characters.
    #[error("invalid hex encoding in hash string")]
    InvalidHexEncoding(#[from] hex::FromHexError),
}

#[cfg(test)]
mod tests {
    use super::{Hash, HashError};

    #[test]
    fn hashing() {
        let hash = Hash::new([1, 2, 3]);

        assert_eq!(
            hash.as_bytes(),
            &[
                177, 119, 236, 27, 242, 109, 251, 59, 112, 16, 212, 115, 230, 212, 71, 19, 178,
                155, 118, 91, 153, 198, 230, 14, 203, 250, 231, 66, 222, 73, 101, 67
            ]
        );
    }

    #[test]
    fn serialize() {
        // Serialize CBOR (non human-readable byte encoding)
        let mut bytes: Vec<u8> = Vec::new();
        let hash = Hash::new([1, 2, 3]);
        ciborium::ser::into_writer(&hash, &mut bytes).unwrap();
        assert_eq!(
            bytes,
            vec![
                88, 32, 177, 119, 236, 27, 242, 109, 251, 59, 112, 16, 212, 115, 230, 212, 71, 19,
                178, 155, 118, 91, 153, 198, 230, 14, 203, 250, 231, 66, 222, 73, 101, 67
            ]
        );

        // Serialize JSON (human-readable hex encoding)
        let json = serde_json::to_string(&hash).unwrap();
        assert_eq!(
            json,
            "\"b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543\""
        );
    }

    #[test]
    fn deserialize() {
        // Deserialize CBOR (non human-readable byte encoding)
        let bytes = [
            88, 32, 177, 119, 236, 27, 242, 109, 251, 59, 112, 16, 212, 115, 230, 212, 71, 19, 178,
            155, 118, 91, 153, 198, 230, 14, 203, 250, 231, 66, 222, 73, 101, 67,
        ];
        let hash: Hash = ciborium::de::from_reader(&bytes[..]).unwrap();
        assert_eq!(hash, Hash::new([1, 2, 3]));

        // Deserialize JSON (human-readable hex encoding)
        let json = "\"b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543\"";
        let hash: Hash = serde_json::from_str(json).unwrap();
        assert_eq!(hash, Hash::new([1, 2, 3]));
    }

    #[test]
    fn invalid_length() {
        let bytes = vec![254, 100, 4, 7];
        let result: Result<Hash, HashError> = bytes.as_slice().try_into();
        matches!(result, Err(HashError::InvalidLength(4, 32)));
    }

    #[test]
    fn invalid_hex_encoding() {
        let hex = "notreallyahexstring";
        let result: Result<Hash, HashError> = hex.parse();
        matches!(
            result,
            Err(HashError::InvalidHexEncoding(
                hex::FromHexError::InvalidHexCharacter { c: 'n', index: 0 }
            ))
        );
    }
}
