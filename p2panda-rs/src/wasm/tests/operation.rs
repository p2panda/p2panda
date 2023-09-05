// SPDX-License-Identifier: AGPL-3.0-or-later

use js_sys::{Array, JSON};
use serde_bytes::ByteBuf;
use serde_wasm_bindgen::to_value;
use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

use crate::hash::Hash;
use crate::operation::EncodedOperation;
use crate::test_utils::fixtures::{operation_with_schema, random_document_view_id};
use crate::wasm::serde::deserialize_from_js;
use crate::wasm::{decode_operation, encode_operation, OperationFields, PlainOperation};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn add_operation_fields() {
    let mut fields = OperationFields::new();

    let relation =
        JsValue::from_str("00205d23607adf6490033cc319cd2b193b2674243f7dd56912432978684ed4fbf12e");

    let list = Array::new();
    list.push(&relation);

    let bytes = to_value(&[0, 1, 2, 3]).unwrap();

    // Add a couple of valid fields
    fields
        .insert("name", "str", JsValue::from_str("Panda"))
        .unwrap();

    fields
        .insert("is_panda", "bool", JsValue::from_bool(true))
        .unwrap();

    fields.insert("data", "bytes", bytes).unwrap();

    fields
        .insert("height_cm", "float", JsValue::from_f64(167.8))
        .unwrap();

    fields
        .insert("favorite_cafe", "relation", relation.clone())
        .unwrap();

    fields
        .insert("locations", "relation_list", list.clone().into())
        .unwrap();

    // Make sure they have been added successfully
    assert_eq!(fields.get("name").unwrap(), "Panda");
    assert_eq!(fields.get("is_panda").unwrap(), true);

    let value_bytes: ByteBuf = deserialize_from_js(fields.get("data").unwrap()).unwrap();
    assert_eq!(value_bytes.to_vec(), vec![0, 1, 2, 3]);
    assert_eq!(fields.get("height_cm").unwrap(), 167.8);

    // Note: A `==` comparison of two "equal" objects will still result in `false` in JavaScript,
    // this is why we compare the string representations instead.
    assert_eq!(
        fields.get("favorite_cafe").unwrap().as_string(),
        relation.as_string()
    );
    assert_eq!(
        fields.get("locations").unwrap().as_string(),
        list.as_string()
    );

    // Check if number of fields is correct
    assert_eq!(fields.len(), 6);
}

#[wasm_bindgen_test]
fn invalid_relation_values() {
    let mut fields = OperationFields::new();

    // Fail when passing invalid JavaScript object
    let unknown_object = JSON::parse(
        "{
            \"unknown_field\": \"00205d23607adf6490033cc319cd2b193b2674243f7dd56912432978684ed4fbf12e\",
            \"this_field\": \"is_not_known!\"
        }"
    ).unwrap();

    assert!(fields
        .insert("test", "relation", unknown_object.clone())
        .is_err());

    let list = Array::new();
    list.push(&unknown_object);
    assert!(fields.insert("test", "relation_list", list.into()).is_err());

    // Fail when using invalid hash
    let invalid_hash_1 = JsValue::from_str("this is not a hash");
    assert!(fields
        .insert("test", "relation", invalid_hash_1.clone())
        .is_err());

    let list = Array::new();
    list.push(&invalid_hash_1);
    assert!(fields.insert("test", "relation_list", list.into()).is_err());
}

#[wasm_bindgen_test]
fn integer_fields() {
    let mut fields = OperationFields::new();

    // "int" fields get added as strings to allow large integers
    fields.insert("age", "int", JsValue::from_str("5")).unwrap();

    // "int" fields always get returned as BigInt instances
    assert_eq!(fields.get("age").unwrap(), JsValue::bigint_from_str("5"));
}

#[wasm_bindgen_test]
fn large_integers() {
    let mut fields = OperationFields::new();

    fields
        .insert(
            "really_big_number",
            "int",
            JsValue::from_str("3147483647345534523"),
        )
        .unwrap();

    assert_eq!(
        fields.get("really_big_number").unwrap(),
        JsValue::bigint_from_str("3147483647345534523")
    );

    // This integer is too large and can't be represented as i64
    let result = fields.insert(
        "really_big_number",
        "int",
        JsValue::from_str("932187321673219932187732188"),
    );
    assert!(result.is_err());
}

#[wasm_bindgen_test]
fn encodes_operations() {
    let mut fields = OperationFields::new();

    let relation =
        JsValue::from_str("00205d23607adf6490033cc319cd2b193b2674243f7dd56912432978684ed4fbf12e");

    let bytes = to_value(&[0, 1, 2, 3]).unwrap();

    // Create a couple of operation fields
    fields
        .insert("name", "str", JsValue::from_str("Panda"))
        .unwrap();

    fields
        .insert("is_panda", "bool", JsValue::from_bool(true))
        .unwrap();

    fields.insert("data", "bytes", bytes).unwrap();

    fields.insert("age", "int", JsValue::from_str("5")).unwrap();

    fields
        .insert("height_cm", "float", JsValue::from_f64(167.8))
        .unwrap();

    fields
        .insert("favorite_cafe", "relation", relation)
        .unwrap();

    // ~~~~~~
    // CREATE
    // ~~~~~~

    let hash = Hash::new_from_bytes(&[1, 2, 3]);
    let schema_id = format!("test_{}", hash.as_str());

    // Encode as CREATE operation
    let create_operation = encode_operation(
        0,
        schema_id.clone(),
        JsValue::UNDEFINED,
        Some(fields.clone()),
    );
    assert!(create_operation.is_ok());

    // ~~~~~~
    // UPDATE
    // ~~~~~~

    // Get hash from CREATE operation
    let document_id = EncodedOperation::from_hex(&create_operation.unwrap()).hash();

    // Encode another UPDATE operation and refer to previous CREATE operation
    let previous = Array::new();
    previous.push(&JsValue::from_str(document_id.as_str()));

    let update_operation = encode_operation(1, schema_id.clone(), previous.into(), Some(fields));
    assert!(update_operation.is_ok());

    // ~~~~~~
    // DELETE
    // ~~~~~~

    // Get hash from UPDATE operation
    let update_op_hash = EncodedOperation::from_hex(&update_operation.unwrap()).hash();

    // Encode another DELETE operation and refer to previous UPDATE operation
    let previous = Array::new();
    previous.push(&JsValue::from_str(update_op_hash.as_str()));

    let delete_operation = encode_operation(2, schema_id, previous.into(), None);
    assert!(delete_operation.is_ok());
}

#[wasm_bindgen_test]
fn decode_operations() {
    // Prepare operation
    let mut fields = crate::operation::OperationFields::new();
    fields.insert("data", [0, 1, 2, 3].as_ref().into()).unwrap();
    fields.insert("username", "dolphin".into()).unwrap();
    let operation = operation_with_schema(Some(fields), Some(random_document_view_id()));
    let operation_encoded = crate::operation::encode::encode_operation(&operation).unwrap();

    // Decode operation
    let result = decode_operation(operation_encoded.to_string()).unwrap();
    let plain_operation: PlainOperation = deserialize_from_js(result).unwrap();
    //     assert_eq!(plain_operation.version, 1);
    //     assert_eq!(plain_operation.action, 1);
    //
    //     let plain_fields = plain_operation.fields.unwrap();
    //     assert_eq!(plain_fields.len(), 2);
    //
    //     let value_bytes: ByteBuf = deserialize_from_js(plain_fields.get("data").unwrap()).unwrap();
    //     assert_eq!(value_bytes.to_vec(), vec![0, 1, 2, 3]);
    //
    //     let value_string: String = deserialize_from_js(plain_fields.get("username").unwrap()).unwrap();
    //     assert_eq!(value_string, "dolphin");
}
