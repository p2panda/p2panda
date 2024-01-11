// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Display;
use std::hash::Hash as StdHash;

use serde::{Deserialize, Serialize};

use crate::hash::Hash;
use crate::operation::header::decode::decode_header;
use crate::serde::{deserialize_hex, serialize_hex};

use super::encode::encode_header;

#[derive(Clone, Debug, PartialEq, Eq, StdHash, Serialize, Deserialize)]
pub struct EncodedHeader(
    #[serde(serialize_with = "serialize_hex", deserialize_with = "deserialize_hex")] Vec<u8>,
);

impl EncodedHeader {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self(bytes.to_owned())
    }

    pub fn hash(&self) -> Hash {
        Hash::new_from_bytes(&self.0)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.0.clone()
    }

    pub fn size(&self) -> u64 {
        self.0.len() as u64
    }

    pub fn unsigned_bytes(&self) -> Vec<u8> {
        // Safely unwrap as only valid headers are encoded.
        let mut header = decode_header(self).unwrap();

        // Set the signature to None
        header.10 = None;

        // Encode again, unwrapping safely as this header is still valid.
        encode_header(&header).unwrap().to_bytes()
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.to_bytes())
    }
}

impl Display for EncodedHeader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(&self.0))
    }
}

#[cfg(any(feature = "test-utils", test))]
impl EncodedHeader {
    pub fn new(bytes: &[u8]) -> EncodedHeader {
        Self(bytes.to_owned())
    }

    pub fn from_hex(value: &str) -> EncodedHeader {
        let bytes = hex::decode(value).expect("invalid hexadecimal value");
        Self(bytes)
    }
}
