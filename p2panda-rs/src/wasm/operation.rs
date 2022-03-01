// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;

use js_sys::Array;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;

use crate::hash::Hash;
use crate::operation::{
    Operation, OperationEncoded, OperationFields as OperationFieldsNonWasm, OperationValue,
    Relation, RelationList,
};
use crate::schema::SchemaId;
use crate::wasm::error::jserr;
use crate::wasm::serde::{deserialize_from_js, serialize_to_js};
use crate::Validate;

/// Use `OperationFields` to attach application data to a [`Operation`].
///
/// See [`crate::atomic::OperationFields`] for further documentation.
#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct OperationFields(OperationFieldsNonWasm);

#[wasm_bindgen]
impl OperationFields {
    /// Returns an `OperationFields` instance.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self(OperationFieldsNonWasm::new())
    }

    /// Adds a field with a value and a given value type.
    ///
    /// The type is defined by a simple string, similar to an enum. Since Rust enums can not (yet)
    /// be exported via wasm-bindgen we have to do it like this. Possible type values are "str"
    /// (String), "bool" (Boolean), "float" (Number), "relation" (String representing a hex-encoded
    /// hash) and "int" (Number).
    ///
    /// This method will throw an error when the field was already set, an invalid type value got
    /// passed or when the value does not reflect the given type.
    #[wasm_bindgen]
    pub fn add(&mut self, name: &str, value_type: &str, value: JsValue) -> Result<(), JsValue> {
        match value_type {
            "str" => {
                let value_str = jserr!(value.as_string().ok_or("Invalid string value"));
                jserr!(self.0.add(name, OperationValue::Text(value_str)));
                Ok(())
            }
            "bool" => {
                let value_bool = jserr!(value.as_bool().ok_or("Invalid boolean value"));
                jserr!(self.0.add(name, OperationValue::Boolean(value_bool)));
                Ok(())
            }
            "int" => {
                // We expect a string here instead of a number, to assure we can pass large numbers
                // coming from the JavaScript world.
                //
                // The largest JavaScript integer is 53 bits but we support 64 bits in the
                // protocol.
                let value_str = jserr!(value.as_string().ok_or("Must be passed as a string"));
                let value_int: i64 = jserr!(value_str.parse(), "Invalid integer value");
                jserr!(self.0.add(name, OperationValue::Integer(value_int)));
                Ok(())
            }
            "float" => {
                let value_float = jserr!(value.as_f64().ok_or("Invalid float value"));
                jserr!(self.0.add(name, OperationValue::Float(value_float)));
                Ok(())
            }
            "relation" => {
                let relation: Relation = jserr!(deserialize_from_js(value), "Invalid object");
                jserr!(relation.validate());
                jserr!(self.0.add(name, OperationValue::Relation(relation)));
                Ok(())
            }
            "relation_list" => {
                let relations: RelationList = jserr!(deserialize_from_js(value), "Invalid array");
                for relation in &relations {
                    jserr!(relation.validate());
                }
                jserr!(self.0.add(name, OperationValue::RelationList(relations)));
                Ok(())
            }
            _ => Err(js_sys::Error::new("Unknown value type").into()),
        }
    }

    /// Removes an existing field from this `OperationFields` instance.
    ///
    /// This might throw an error when trying to remove an nonexistent field.
    #[wasm_bindgen]
    pub fn remove(&mut self, name: &str) -> Result<(), JsValue> {
        jserr!(self.0.remove(name));
        Ok(())
    }

    /// Returns field of this `OperationFields` instance when existing.
    #[wasm_bindgen]
    pub fn get(&mut self, name: &str) -> Result<JsValue, JsValue> {
        match self.0.get(name) {
            Some(OperationValue::Boolean(value)) => Ok(JsValue::from_bool(value.to_owned())),
            Some(OperationValue::Text(value)) => Ok(JsValue::from_str(value)),
            Some(OperationValue::Relation(value)) => Ok(jserr!(serialize_to_js(value))),
            Some(OperationValue::RelationList(value)) => Ok(jserr!(serialize_to_js(value))),
            Some(OperationValue::Float(value)) => Ok(JsValue::from_f64(value.to_owned())),
            Some(OperationValue::Integer(value)) => Ok(JsValue::from(value.to_owned())),
            None => Ok(JsValue::NULL),
        }
    }

    /// Returns the number of fields in this instance.
    #[wasm_bindgen(js_name = length)]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns true when no field exists.
    #[wasm_bindgen(js_name = isEmpty)]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns this instance formatted for debugging.
    #[wasm_bindgen(js_name = toString)]
    #[allow(clippy::inherent_to_string)]
    pub fn to_string(&self) -> String {
        format!("{:?}", self)
    }
}

impl Default for OperationFields {
    fn default() -> Self {
        Self::new()
    }
}

/// Returns an encoded CREATE operation that creates a document of the provided schema.
#[wasm_bindgen(js_name = encodeCreateOperation)]
pub fn encode_create_operation(
    schema_hash: String,
    fields: OperationFields,
) -> Result<String, JsValue> {
    let schema = jserr!(SchemaId::new(&schema_hash));
    let operation = jserr!(Operation::new_create(schema, fields.0));
    let operation_encoded = jserr!(OperationEncoded::try_from(&operation));
    Ok(operation_encoded.as_str().to_owned())
}

/// Returns an encoded UPDATE operation that updates fields of a given document.
#[wasm_bindgen(js_name = encodeUpdateOperation)]
pub fn encode_update_operation(
    schema_hash: String,
    previous_operations: Array,
    fields: OperationFields,
) -> Result<String, JsValue> {
    let schema = jserr!(SchemaId::new(&schema_hash));

    // Decode JsValue into vector of strings
    let prev_op_strings: Vec<String> = jserr!(previous_operations.into_serde());

    // Create hashes from strings and collect wrapped in a result
    let prev_op_result: Result<Vec<Hash>, _> = prev_op_strings
        .iter()
        .map(|prev_op| Hash::new(prev_op))
        .collect();

    let previous = jserr!(prev_op_result);
    let operation = jserr!(Operation::new_update(schema, previous, fields.0));
    let operation_encoded = jserr!(OperationEncoded::try_from(&operation));
    Ok(operation_encoded.as_str().to_owned())
}

/// Returns an encoded DELETE operation that deletes a given document.
#[wasm_bindgen(js_name = encodeDeleteOperation)]
pub fn encode_delete_operation(
    schema_hash: String,
    previous_operations: Array,
) -> Result<String, JsValue> {
    let schema = jserr!(SchemaId::new(&schema_hash));

    // Decode JsValue into vector of strings
    let prev_op_strings: Vec<String> = jserr!(previous_operations.into_serde());

    // Create hashes from strings and collect wrapped in a result
    let prev_op_result: Result<Vec<Hash>, _> = prev_op_strings
        .iter()
        .map(|prev_op| Hash::new(prev_op))
        .collect();

    let previous = jserr!(prev_op_result);
    let operation = jserr!(Operation::new_delete(schema, previous));
    let operation_encoded = jserr!(OperationEncoded::try_from(&operation));
    Ok(operation_encoded.as_str().to_owned())
}
