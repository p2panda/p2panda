// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::ser::Serialize;
use serde_wasm_bindgen::{Error, Serializer};
use wasm_bindgen::JsValue;

/// Serialize any struct into JavaScript values.
///
/// Note that this will NOT serialize into a JSON string but an actual JavaScript object
/// (potentially with non-JSON values inside like "Map" or "BigInt").
///
/// This method uses the `serde_wasm_bindgen` serializer, both for bundle size but also for its
/// support of large number types. With this serializer all i64 and u64 numbers get serialized as
/// BigInt instances.
pub fn serialize_to_js<T>(value: &T) -> Result<JsValue, Error>
where
    T: Serialize + ?Sized,
{
    let serializer = Serializer::new().serialize_large_number_types_as_bigints(true);
    value.serialize(&serializer)
}
