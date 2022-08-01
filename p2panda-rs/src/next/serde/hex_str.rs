// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

/// Helper method for `serde` to serialize bytes into a hex string when using a human readable
/// encoding (JSON), otherwise it serializes the bytes directly.
pub fn serialize_hex<S>(value: &[u8], serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    if serializer.is_human_readable() {
        hex::serde::serialize(value, serializer)
    } else {
        value.serialize(serializer)
    }
}

/// Helper method for `serde` to deserialize from a hex string into bytes when using a human
/// readable encoding (JSON), otherwise it deserializes the bytes directly.
pub fn deserialize_hex<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    if deserializer.is_human_readable() {
        hex::serde::deserialize(deserializer)
    } else {
        let bytes: Vec<u8> = Deserialize::deserialize(deserializer)?;
        Ok(bytes)
    }
}
