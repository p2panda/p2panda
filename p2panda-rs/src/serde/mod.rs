// SPDX-License-Identifier: AGPL-3.0-or-later

//! Commonly used functions for serialization and deserialization of values.
#[cfg(any(feature = "test-utils", test))]
mod cbor;
mod hex_str;
mod u64_str;

#[cfg(any(feature = "test-utils", test))]
pub use cbor::{deserialize_into, serialize_from, serialize_value};
#[cfg(any(feature = "test-utils", test))]
pub use hex_str::hex_string_to_bytes;
pub use hex_str::{deserialize_hex, serialize_hex_bytes, serialize_hex_string};
pub use u64_str::StringOrU64;
