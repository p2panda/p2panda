// SPDX-License-Identifier: AGPL-3.0-or-later

use js_sys::{Array, JSON};
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

    let relation =
        JsValue::from_str("00205d23607adf6490033cc319cd2b193b2674243f7dd56912432978684ed4fbf12e");

    let list = Array::new();
    list.push(&relation);

    // Add a couple of valid fields
    fields
        .add("name", "str", JsValue::from_str("Panda"))
        .unwrap();

    fields
        .add("is_panda", "bool", JsValue::from_bool(true))
        .unwrap();

    fields
        .add("height_cm", "float", JsValue::from_f64(167.8))
        .unwrap();

    fields
        .add("favorite_cafe", "relation", relation.clone())
        .unwrap();

    fields
        .add("locations", "relation_list", list.clone().into())
        .unwrap();

    // Make sure they have been added successfully
    assert_eq!(fields.get("name").unwrap(), "Panda");
    assert_eq!(fields.get("is_panda").unwrap(), true);
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
    assert_eq!(fields.len(), 5);

    // .. and remove them again successfully
    fields.remove("name").unwrap();
    fields.remove("is_panda").unwrap();
    fields.remove("height_cm").unwrap();
    fields.remove("favorite_cafe").unwrap();
    fields.remove("locations").unwrap();
    assert_eq!(fields.len(), 0);
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
        .add("test", "relation", unknown_object.clone())
        .is_err());

    let list = Array::new();
    list.push(&unknown_object);
    assert!(fields.add("test", "relation_list", list.into()).is_err());

    // Fail when using invalid hash
    let invalid_hash_1 = JsValue::from_str("this is not a hash");
    assert!(fields
        .add("test", "relation", invalid_hash_1.clone())
        .is_err());

    let list = Array::new();
    list.push(&invalid_hash_1);
    assert!(fields.add("test", "relation_list", list.into()).is_err());
}

#[wasm_bindgen_test]
fn inexistent_fields() {
    let mut fields = OperationFields::new();
    let result = fields.remove("non_existant_key");
    assert!(result.is_err());
}

#[wasm_bindgen_test]
fn integer_fields() {
    let mut fields = OperationFields::new();

    // "int" fields get added as strings to allow large integers
    fields.add("age", "int", JsValue::from_str("5")).unwrap();

    // "int" fields always get returned as BigInt instances
    assert_eq!(fields.get("age").unwrap(), JsValue::bigint_from_str("5"));
}

#[wasm_bindgen_test]
fn large_integers() {
    let mut fields = OperationFields::new();

    fields
        .add(
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
    let result = fields.add(
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

    // Create a couple of operation fields
    fields
        .add("name", "str", JsValue::from_str("Panda"))
        .unwrap();

    fields
        .add("is_panda", "bool", JsValue::from_bool(true))
        .unwrap();

    fields.add("age", "int", JsValue::from_str("5")).unwrap();

    fields
        .add("height_cm", "float", JsValue::from_f64(167.8))
        .unwrap();

    fields.add("favorite_cafe", "relation", relation).unwrap();

    // ~~~~~~
    // CREATE
    // ~~~~~~

    let hash = Hash::new_from_bytes(vec![1, 2, 3]).unwrap();
    let schema = JsValue::from_str(&format!("test_{}", hash.as_str()));

    // Encode as CREATE operation
    let create_operation = encode_create_operation(schema.clone(), fields.clone());

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
        encode_update_operation(schema.clone(), previous_operations.into(), fields);

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

    let delete_operation = encode_delete_operation(schema, previous_operations.into());

    assert!(delete_operation.is_ok());
}
