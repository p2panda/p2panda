// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::de::DeserializeOwned;
use serde::ser::Serialize;
use serde_wasm_bindgen::{Deserializer, Error, Serializer};
use wasm_bindgen::JsValue;

/// Serializes Rust type into JavaScript value (`JsValue`).
///
/// Note that this will NOT serialize into a JSON string but an actual JavaScript object
/// (potentially with non-JSON values inside like "Map" or "BigInt").
///
/// This method uses the `serde_wasm_bindgen` serializer, both for bundle size but also for its
/// support of large number types. With this serializer all i64 and u64 numbers get serialized as
/// BigInt instances.
pub fn serialize_to_js<T: Serialize + ?Sized>(value: &T) -> Result<JsValue, Error> {
    let serializer = Serializer::new()
        .serialize_large_number_types_as_bigints(true)
        .serialize_bytes_as_arrays(false);
    let output = value.serialize(&serializer)?;
    Ok(output)
}

/// Converts JavaScript value (`JsValue`) into Rust type.
pub fn deserialize_from_js<T: DeserializeOwned>(value: JsValue) -> Result<T, JsValue> {
    let value = T::deserialize(Deserializer::from(value))?;
    Ok(value)
}
