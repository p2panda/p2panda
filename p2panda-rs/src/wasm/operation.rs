// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;

use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;

use crate::operation::{
    Operation, OperationEncoded, OperationFields as OperationFieldsNonWasm, OperationId,
    OperationValue, PinnedRelation, PinnedRelationList, Relation, RelationList,
};
use crate::schema::SchemaId;
use crate::schema::key_group::Owner;
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
    /// The type is defined by a simple string, similar to an enum. Possible type values are:
    ///
    /// - "bool" (Boolean)
    /// - "float" (Number)
    /// - "int" (Number)
    /// - "str" (String)
    /// - "relation" (hex-encoded document id)
    /// - "relation_list" (array of hex-encoded document ids)
    /// - "pinned_relation" (document view id, represented as an array
    ///     of hex-encoded operation ids)
    /// - "pinned_relation_list" (array of document view ids, represented as an array
    ///     of arrays of hex-encoded operation ids)
    /// - "owner" (hex-encoded document id)
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
                let relation: Relation = jserr!(
                    deserialize_from_js(value),
                    "Expected an operation id value for field of type relation"
                );
                jserr!(relation.validate());
                jserr!(self.0.add(name, OperationValue::Relation(relation)));
                Ok(())
            }
            "relation_list" => {
                let relations: RelationList = jserr!(
                    deserialize_from_js(value),
                    "Expected an array of operation ids for field of type relation list"
                );
                jserr!(relations.validate());
                jserr!(self.0.add(name, OperationValue::RelationList(relations)));
                Ok(())
            }
            "pinned_relation" => {
                let relation: PinnedRelation = jserr!(
                    deserialize_from_js(value),
                    "Expected an array of operation ids for field of type pinned relation list"
                );
                jserr!(relation.validate());
                jserr!(self.0.add(name, OperationValue::PinnedRelation(relation)));
                Ok(())
            }
            "pinned_relation_list" => {
                let relations: PinnedRelationList = jserr!(
                    deserialize_from_js(value),
                    "Expected a nested array of operation ids for field of type pinned relation list"
                );
                jserr!(relations.validate());
                jserr!(self
                    .0
                    .add(name, OperationValue::PinnedRelationList(relations)));
                Ok(())
            }
            "owner" => {
                let relation: Relation = jserr!(
                    deserialize_from_js(value),
                    "Expected an operation id value for field of type owner"
                );
                jserr!(relation.validate());
                jserr!(self.0.add(name, OperationValue::Owner(relation)));
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
            Some(OperationValue::PinnedRelation(value)) => Ok(jserr!(serialize_to_js(value))),
            Some(OperationValue::PinnedRelationList(value)) => Ok(jserr!(serialize_to_js(value))),
            Some(OperationValue::Float(value)) => Ok(JsValue::from_f64(value.to_owned())),
            Some(OperationValue::Integer(value)) => Ok(JsValue::from(value.to_owned())),
            Some(OperationValue::Owner(value)) => Ok(jserr!(serialize_to_js(value))),
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
    schema_id: JsValue,
    fields: OperationFields,
) -> Result<String, JsValue> {
    let schema: SchemaId = jserr!(deserialize_from_js(schema_id), "Invalid schema id");
    let operation = jserr!(Operation::new_create(schema, fields.0));
    let operation_encoded = jserr!(OperationEncoded::try_from(&operation));
    Ok(operation_encoded.as_str().to_owned())
}

/// Returns an encoded UPDATE operation that updates fields of a given document.
#[wasm_bindgen(js_name = encodeUpdateOperation)]
pub fn encode_update_operation(
    schema_id: JsValue,
    previous_operations: JsValue,
    fields: OperationFields,
) -> Result<String, JsValue> {
    let schema: SchemaId = jserr!(deserialize_from_js(schema_id), "Invalid schema id");

    // Decode JsValue into vector of strings
    let prev_op_strings: Vec<String> = jserr!(
        deserialize_from_js(previous_operations),
        "Can not deserialize array"
    );

    // Create operation ids from strings and collect wrapped in a result
    let prev_op_result: Result<Vec<OperationId>, _> = prev_op_strings
        .iter()
        .map(|prev_op| prev_op.parse())
        .collect();

    let previous = jserr!(prev_op_result);
    let operation = jserr!(Operation::new_update(schema, previous, fields.0));
    let operation_encoded = jserr!(OperationEncoded::try_from(&operation));
    Ok(operation_encoded.as_str().to_owned())
}

/// Returns an encoded DELETE operation that deletes a given document.
#[wasm_bindgen(js_name = encodeDeleteOperation)]
pub fn encode_delete_operation(
    schema_id: JsValue,
    previous_operations: JsValue,
) -> Result<String, JsValue> {
    let schema: SchemaId = jserr!(deserialize_from_js(schema_id), "Invalid schema id");

    // Decode JsValue into vector of strings
    let prev_op_strings: Vec<String> = jserr!(
        deserialize_from_js(previous_operations),
        "Can not deserialize array"
    );

    // Create operation ids from strings and collect wrapped in a result
    let prev_op_result: Result<Vec<OperationId>, _> = prev_op_strings
        .iter()
        .map(|prev_op| prev_op.parse())
        .collect();

    let previous = jserr!(prev_op_result);
    let operation = jserr!(Operation::new_delete(schema, previous));
    let operation_encoded = jserr!(OperationEncoded::try_from(&operation));
    Ok(operation_encoded.as_str().to_owned())
}
