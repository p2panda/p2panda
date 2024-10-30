// SPDX-License-Identifier: AGPL-3.0-or-later

// @TODO: Module should be renamed
use anyhow::Result;
use p2panda_core::cbor::{decode_cbor, encode_cbor};
use serde::de::DeserializeOwned;
use serde::Serialize;

/// Convert a value to bytes.
pub trait ToBytes {
    fn to_bytes(&self) -> Vec<u8>;
}

/// Convert bytes to a value.
pub trait FromBytes<T> {
    fn from_bytes(bytes: &[u8]) -> Result<T>;
}

impl<T: Serialize> ToBytes for T {
    fn to_bytes(&self) -> Vec<u8> {
        encode_cbor(&self).expect("type can be serialized")
    }
}

impl<T: DeserializeOwned> FromBytes<T> for T {
    fn from_bytes(bytes: &[u8]) -> Result<T> {
        let value = decode_cbor(&bytes[..])?;
        Ok(value)
    }
}
