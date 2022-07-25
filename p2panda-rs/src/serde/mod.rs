// SPDX-License-Identifier: AGPL-3.0-or-later

//! Commonly used functions for serialization and deserialization of values.
mod hex_str;
mod u64_str;

pub use hex_str::{serialize_hex, deserialize_hex};
pub use u64_str::StringOrU64;
