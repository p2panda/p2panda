// SPDX-License-Identifier: AGPL-3.0-or-later

use js_sys::Array;
use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

use crate::hash::Hash;
use crate::operation::OperationEncoded;
use crate::wasm::{
    encode_create_operation, encode_delete_operation, encode_update_operation, OperationFields,
};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn add_remove_operation_fields() {
    let mut fields = OperationFields::new();

    // Add a couple of valid fields
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

    // Make sure they have been added successfully
    assert_eq!(fields.get("name".to_string()).unwrap(), "Panda");
    assert_eq!(fields.get("is_panda".to_string()).unwrap(), true);
    assert_eq!(fields.get("height_cm".to_string()).unwrap(), 167.8);
    assert_eq!(
        fields.get("favorite_cafe".to_string()).unwrap(),
        Hash::new_from_bytes(vec![1, 2, 3]).unwrap().as_str()
    );
    assert_eq!(fields.len(), 4);

    // .. and remove them again successfully
    fields.remove("name".to_string()).unwrap();
    fields.remove("is_panda".to_string()).unwrap();
    fields.remove("height_cm".to_string()).unwrap();
    fields.remove("favorite_cafe".to_string()).unwrap();
    assert_eq!(fields.len(), 0);
}

#[wasm_bindgen_test]
fn inexistent_fields() {
    let mut fields = OperationFields::new();
    let result = fields.remove("non_existant_key".to_string());
    assert!(result.is_err());
}

#[wasm_bindgen_test]
fn integer_fields() {
    let mut fields = OperationFields::new();

    // "int" fields get added as strings to allow large integers
    fields
        .add("age".to_string(), "int".to_string(), JsValue::from_str("5"))
        .unwrap();

    // "int" fields always get returned as BigInt instances
    assert_eq!(
        fields.get("age".to_string()).unwrap(),
        JsValue::bigint_from_str("5")
    );
}

#[wasm_bindgen_test]
fn large_integers() {
    let mut fields = OperationFields::new();

    fields
        .add(
            "really_big_number".to_string(),
            "int".to_string(),
            JsValue::from_str("3147483647345534523"),
        )
        .unwrap();

    assert_eq!(
        fields.get("really_big_number".to_string()).unwrap(),
        JsValue::bigint_from_str("3147483647345534523")
    );

    // This integer is too large and can't be represented as i64
    let result = fields.add(
        "really_big_number".to_string(),
        "int".to_string(),
        JsValue::from_str("932187321673219932187732188"),
    );
    assert!(result.is_err());
}

#[wasm_bindgen_test]
fn encodes_operations() {
    let mut fields = OperationFields::new();

    // Create a couple of operation fields
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
        .add("age".to_string(), "int".to_string(), JsValue::from_str("5"))
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

    // ~~~~~~
    // CREATE
    // ~~~~~~

    let schema = Hash::new_from_bytes(vec![1, 2, 3]).unwrap();

    // Encode as CREATE operation
    let create_operation = encode_create_operation(schema.as_str().into(), fields.clone());

    assert!(create_operation.is_ok());

    // ~~~~~~
    // UPDATE
    // ~~~~~~

    // Get hash from CREATE operation
    let document_id = OperationEncoded::new(&create_operation.unwrap())
        .unwrap()
        .hash();

    // Encode another UPDATE operation and refer to previous CREATE operation
    let previous_operations = Array::new();
    previous_operations.push(&JsValue::from_str(document_id.as_str()));

    let update_operation =
        encode_update_operation(schema.as_str().into(), previous_operations, fields);

    assert!(update_operation.is_ok());

    // ~~~~~~
    // DELETE
    // ~~~~~~

    // Get hash from UPDATE operation
    let update_op_hash = OperationEncoded::new(&update_operation.unwrap())
        .unwrap()
        .hash();

    // Encode another DELETE operation and refer to previous UPDATE operation
    let previous_operations = Array::new();
    previous_operations.push(&JsValue::from_str(update_op_hash.as_str()));

    let delete_operation = encode_delete_operation(schema.as_str().into(), previous_operations);

    assert!(delete_operation.is_ok());
}
