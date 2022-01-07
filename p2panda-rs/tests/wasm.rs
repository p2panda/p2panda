// SPDX-License-Identifier: AGPL-3.0-or-later
#![cfg(target_arch = "wasm32")]

//! Tests for `wasm` module in `p2panda_rs`.
use js_sys::Array;
use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

use p2panda_rs::hash::Hash;
use p2panda_rs::operation::OperationEncoded;
use p2panda_rs::wasm::{
    encode_create_operation, encode_delete_operation, encode_update_operation, OperationFields,
};

#[wasm_bindgen_test]
fn operation_fields() {
    let mut fields = OperationFields::new();
    fields
        .add(
            "name".to_string(),
            "str".to_string(),
            JsValue::from_str("Panda"),
        )
        .unwrap();

    fields
        .add(
            "is_panda".to_string(),
            "bool".to_string(),
            JsValue::from_bool(true),
        )
        .unwrap();

    fields
        .add("age".to_string(), "int".to_string(), JsValue::from_f64(5.0))
        .unwrap();

    fields
        .add(
            "height_cm".to_string(),
            "float".to_string(),
            JsValue::from_f64(167.8),
        )
        .unwrap();

    fields
        .add(
            "favorite_cafe".to_string(),
            "relation".to_string(),
            JsValue::from_str(Hash::new_from_bytes(vec![1, 2, 3]).unwrap().as_str()),
        )
        .unwrap();

    assert_eq!(fields.get("name".to_string()).unwrap(), "Panda");
    assert!(fields.get("is_panda".to_string()).unwrap());
    assert_eq!(fields.get("age".to_string()).unwrap(), 5);
    assert_eq!(fields.get("height_cm".to_string()).unwrap(), 167.8);
    assert_eq!(
        fields.get("favorite_cafe".to_string()).unwrap(),
        Hash::new_from_bytes(vec![1, 2, 3]).unwrap().as_str()
    );

    fields
        .add(
            "really_big_number".to_string(),
            "int".to_string(),
            JsValue::from_f64(31474836473453453434524523.0),
        )
        .unwrap();

    assert!(fields.get("really_big_number".to_string()).is_err());

    assert_eq!(fields.len(), 6);

    fields.remove("really_big_number".to_string()).unwrap();

    assert_eq!(fields.len(), 5);

    fields.remove("name".to_string()).unwrap();
    fields.remove("is_panda".to_string()).unwrap();
    fields.remove("age".to_string()).unwrap();
    fields.remove("height_cm".to_string()).unwrap();
    fields.remove("favorite_cafe".to_string()).unwrap();

    assert_eq!(fields.len(), 0);

    let result = fields.remove("non_existant_key".to_string());

    assert!(result.is_err());
}

#[wasm_bindgen_test]
fn encodes_operations() {
    let schema = Hash::new_from_bytes(vec![1, 2, 3]).unwrap();
    let mut fields = OperationFields::new();
    fields
        .add(
            "name".to_string(),
            "str".to_string(),
            JsValue::from_str("Panda"),
        )
        .unwrap();

    fields
        .add(
            "is_panda".to_string(),
            "bool".to_string(),
            JsValue::from_bool(true),
        )
        .unwrap();

    fields
        .add("age".to_string(), "int".to_string(), JsValue::from_f64(5.0))
        .unwrap();

    fields
        .add(
            "height_cm".to_string(),
            "float".to_string(),
            JsValue::from_f64(167.8),
        )
        .unwrap();

    fields
        .add(
            "favorite_cafe".to_string(),
            "relation".to_string(),
            JsValue::from_str(Hash::new_from_bytes(vec![1, 2, 3]).unwrap().as_str()),
        )
        .unwrap();

    let create_operation = encode_create_operation(schema.as_str().into(), fields.clone());

    assert!(create_operation.is_ok());

    let document_id = OperationEncoded::new(&create_operation.unwrap())
        .unwrap()
        .hash();

    let previous_operations = Array::new();
    previous_operations.push(&JsValue::from_str(document_id.as_str()));

    let update_operation = encode_update_operation(
        document_id.as_str().into(),
        schema.as_str().into(),
        previous_operations,
        fields,
    );

    assert!(update_operation.is_ok());

    let update_op_hash = OperationEncoded::new(&update_operation.unwrap())
        .unwrap()
        .hash();

    let previous_operations = Array::new();
    previous_operations.push(&JsValue::from_str(update_op_hash.as_str()));

    let delete_operation = encode_delete_operation(
        document_id.as_str().into(),
        schema.as_str().into(),
        previous_operations,
    );

    assert!(delete_operation.is_ok());
}
