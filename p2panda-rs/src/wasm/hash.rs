// SPDX-License-Identifier: AGPL-3.0-or-later

use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;

use crate::wasm::error::jserr;

/// Returns hash of an hexadecimal encoded value.
#[wasm_bindgen(js_name = generateHash)]
pub fn generate_hash(value: &str) -> Result<String, JsValue> {
    // Convert hexadecimal string to bytes
    let bytes = jserr!(hex::decode(value), "Invalid hex-encoding");

    // Hash the value and return it as a string
    let hash = crate::hash::Hash::new_from_bytes(&bytes);
    Ok(hash.to_string())
}
