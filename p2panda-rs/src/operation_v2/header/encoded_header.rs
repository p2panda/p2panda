// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Display;
use std::hash::Hash as StdHash;

use serde::{Deserialize, Serialize};

use crate::hash_v2::Hash;
use crate::operation_v2::header::traits::AsEncodedHeader;
use crate::serde::{deserialize_hex, serialize_hex};

#[derive(Clone, Debug, PartialEq, Eq, StdHash, Serialize, Deserialize)]
pub struct EncodedHeader(
    #[serde(serialize_with = "serialize_hex", deserialize_with = "deserialize_hex")] Vec<u8>,
);

impl EncodedHeader {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self(bytes.to_owned())
    }
}

impl AsEncodedHeader for EncodedHeader {
    fn hash(&self) -> Hash {
        Hash::new_from_bytes(&self.0)
    }

    fn to_bytes(&self) -> Vec<u8> {
        self.0.clone()
    }

    fn size(&self) -> u64 {
        self.0.len() as u64
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
