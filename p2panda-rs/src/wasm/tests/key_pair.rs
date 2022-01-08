// SPDX-License-Identifier: AGPL-3.0-or-later

use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

use crate::identity::KeyPair as NonWasmKeyPair;
use crate::wasm::{verify_signature, KeyPair};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn verifies_data() {
    // Wasm KeyPair
    let wasm_key_pair = KeyPair::new();

    // Non-wasm KeyPair (derived from wasm KeyPair).
    let key_pair = NonWasmKeyPair::from_private_key_str(&wasm_key_pair.private_key()).unwrap();

    let wasm_public_key = wasm_key_pair.public_key().as_str().to_string();
    let public_key = hex::encode(key_pair.public_key().to_bytes());

    // Public key strings should match.
    assert_eq!(wasm_public_key, public_key);

    let bytes = b"test";
    let byte_string = String::from_utf8(bytes.to_vec()).unwrap();

    let wasm_signature_string = wasm_key_pair.sign(byte_string.clone());
    let signature_string = hex::encode(key_pair.sign(bytes));

    // Signatures should match.
    assert_eq!(wasm_signature_string, signature_string);

    assert_eq!(
        verify_signature(
            wasm_public_key.clone(),
            byte_string.clone(),
            wasm_signature_string.clone()
        )
        .unwrap(),
        JsValue::TRUE
    );
    assert_eq!(
        verify_signature(public_key, byte_string.clone(), signature_string).unwrap(),
        JsValue::TRUE
    );

    // Passing wrong bytes should return false.
    let wrong_bytes = b"poop";
    let wrong_byte_string = String::from_utf8(wrong_bytes.to_vec()).unwrap();

    assert_eq!(
        verify_signature(
            wasm_public_key.clone(),
            wrong_byte_string,
            wasm_signature_string.clone()
        )
        .unwrap(),
        JsValue::FALSE
    );

    // Passing wrong public key should return false.
    let wrong_public_key_string = KeyPair::new().public_key();

    assert_eq!(
        verify_signature(
            wrong_public_key_string,
            byte_string.clone(),
            wasm_signature_string
        )
        .unwrap(),
        JsValue::FALSE
    );

    // Passing wrong signature should return false.
    let wrong_signature = KeyPair::new().sign(byte_string.clone());

    assert_eq!(
        verify_signature(wasm_public_key, byte_string, wrong_signature).unwrap(),
        JsValue::FALSE
    );
}
