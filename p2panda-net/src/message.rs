// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::Result;
use serde::de::DeserializeOwned;
use serde::Serialize;

pub trait ToBytes {
    fn to_bytes(&self) -> Vec<u8>;
}

pub trait FromBytes<T> {
    fn from_bytes(bytes: &[u8]) -> Result<T>;
}

#[cfg(feature = "cbor")]
impl<T: Serialize> ToBytes for T {
    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        ciborium::into_writer(&self, &mut bytes).expect("type can be serialized");
        bytes
    }
}

#[cfg(feature = "cbor")]
impl<T: DeserializeOwned> FromBytes<T> for T {
    fn from_bytes(bytes: &[u8]) -> Result<T> {
        let value = ciborium::from_reader(bytes)?;
        Ok(value)
    }
}
