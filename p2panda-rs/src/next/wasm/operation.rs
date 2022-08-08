// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::{TryFrom, TryInto};
use std::str::FromStr;

use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;

use crate::next::document::error::DocumentViewIdError;
use crate::next::document::{DocumentId, DocumentViewId};
use crate::next::operation::encode::encode_plain_operation;
use crate::next::operation::plain::{PlainFields, PlainOperation, PlainValue};
use crate::next::operation::{OperationAction, OperationId, OperationVersion, RelationList};
use crate::next::schema::SchemaId;
use crate::next::wasm::error::jserr;
use crate::next::wasm::serde::{deserialize_from_js, serialize_to_js};

/// Use `OperationFields` to attach application data to an [`Operation`].
///
// @TODO: This uses plain fields which are schemaless. In the future we want to use regular
// operation fields and a wasm-compatible schema object.
#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct OperationFields(PlainFields);

#[wasm_bindgen]
impl OperationFields {
    /// Returns an `OperationFields` instance.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self(PlainFields::new())
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
    ///
    /// This method will throw an error when the field was already set, an invalid type value got
    /// passed or when the value does not reflect the given type.
    #[wasm_bindgen]
    pub fn insert(&mut self, name: &str, value_type: &str, value: JsValue) -> Result<(), JsValue> {
        match value_type {
            "str" => {
                let value_str = jserr!(value.as_string().ok_or("Invalid string value"));
                jserr!(self.0.insert(name, PlainValue::StringOrRelation(value_str)));
                Ok(())
            }
            "bool" => {
                let value_bool = jserr!(value.as_bool().ok_or("Invalid boolean value"));
                jserr!(self.0.insert(name, PlainValue::Boolean(value_bool)));
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
                jserr!(self.0.insert(name, PlainValue::Integer(value_int)));
                Ok(())
            }
            "float" => {
                let value_float = jserr!(value.as_f64().ok_or("Invalid float value"));
                jserr!(self.0.insert(name, PlainValue::Float(value_float)));
                Ok(())
            }
            "relation" => {
                // Pass document id as a string
                let value_str = jserr!(value
                    .as_string()
                    .ok_or("Expected a document id string for field of type relation"));

                // Convert to `DocumentId` to validate it
                jserr!(
                    DocumentId::from_str(&value_str),
                    "Invalid document id found for relation"
                );

                // @TODO: Actually store a `DocumentId` as soon as we stop using plain operations
                jserr!(self.0.insert(name, PlainValue::StringOrRelation(value_str)));
                Ok(())
            }
            "relation_list" => {
                // Pass as array of strings
                let relations_str: Vec<String> = jserr!(
                    deserialize_from_js(value),
                    "Expected an array of operation ids for field of type relation list"
                );

                // Convert to array of document ids to validate it
                jserr!(
                    RelationList::try_from(relations_str.as_slice()),
                    "Invalid document id found in relation list"
                );

                // @TODO: Actually store an array of `DocumentId` as soon as we don't use plain
                // operations
                jserr!(self.0.insert(
                    name,
                    PlainValue::PinnedRelationOrRelationList(relations_str)
                ));
                Ok(())
            }
            "pinned_relation" => {
                // Pass as array of operation id strings
                let operations_str: Vec<String> = jserr!(
                    deserialize_from_js(value),
                    "Expected an array of operation ids for field of type pinned relation"
                );

                // Convert to document view id to validate it
                jserr!(
                    DocumentViewId::try_from(operations_str.as_slice()),
                    "Invalid operation id found in pinned relation"
                );

                // @TODO: Actually store as `DocumentViewId` as soon as we don't use plain
                // operations
                jserr!(self.0.insert(
                    name,
                    PlainValue::PinnedRelationOrRelationList(operations_str)
                ));
                Ok(())
            }
            "pinned_relation_list" => {
                // Pass as array of string arrays
                let relations_str: Vec<Vec<String>> = jserr!(
                    deserialize_from_js(value),
                    "Expected a nested array of operation ids for field of type pinned relation list"
                );

                // Convert to document view ids to validate it
                let document_view_ids: Result<(), DocumentViewIdError> =
                    relations_str.iter().try_for_each(|operation_ids_str| {
                        // Convert list of strings to list of operation ids aka a document view
                        // id, this checks if list of operation ids is sorted and without any
                        // duplicates
                        let _: DocumentViewId = operation_ids_str.as_slice().try_into()?;
                        Ok(())
                    });

                jserr!(
                    document_view_ids,
                    "Invalid document view id found in pinned relation list"
                );

                // @TODO: Actually store as array of `DocumentViewId` as soon as we don't use plain
                // operations
                jserr!(self
                    .0
                    .insert(name, PlainValue::PinnedRelationList(relations_str)));
                Ok(())
            }
            _ => Err(js_sys::Error::new("Unknown value type").into()),
        }
    }

    /// Returns field of this `OperationFields` instance when existing.
    #[wasm_bindgen]
    pub fn get(&mut self, name: &str) -> Result<JsValue, JsValue> {
        match self.0.get(name) {
            Some(PlainValue::Boolean(value)) => Ok(JsValue::from_bool(value.to_owned())),
            Some(PlainValue::Float(value)) => Ok(JsValue::from_f64(value.to_owned())),
            Some(PlainValue::Integer(value)) => Ok(JsValue::from(value.to_owned())),
            Some(PlainValue::StringOrRelation(value)) => Ok(JsValue::from_str(value)),
            Some(PlainValue::PinnedRelationOrRelationList(value)) => {
                Ok(jserr!(serialize_to_js(value)))
            }
            Some(PlainValue::PinnedRelationList(value)) => Ok(jserr!(serialize_to_js(value))),
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
    let schema_id: SchemaId = jserr!(
        deserialize_from_js(schema_id.clone()),
        format!("Invalid schema id: {:?}", schema_id)
    );

    // @TODO: This does not validate if the operation is correct. We can implement it as soon a we
    // use real operations with schemas again
    let plain_operation = PlainOperation::new(
        OperationVersion::V1,
        OperationAction::Create,
        schema_id,
        None,
        Some(fields.0),
    );

    let operation_encoded = jserr!(encode_plain_operation(&plain_operation));
    Ok(operation_encoded.to_string())
}

/// Returns an encoded UPDATE operation that updates fields of a given document.
#[wasm_bindgen(js_name = encodeUpdateOperation)]
pub fn encode_update_operation(
    schema_id: JsValue,
    previous_operations: JsValue,
    fields: OperationFields,
) -> Result<String, JsValue> {
    let schema_id: SchemaId = jserr!(deserialize_from_js(schema_id), "Invalid schema id");

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

    let previous_ops = jserr!(prev_op_result);
    let document_view_id = DocumentViewId::new(&previous_ops);

    // @TODO: This does not validate if the operation is correct. We can implement it as soon a we
    // use real operations with schemas again
    let plain_operation = PlainOperation::new(
        OperationVersion::V1,
        OperationAction::Update,
        schema_id,
        Some(document_view_id),
        Some(fields.0),
    );

    let operation_encoded = jserr!(encode_plain_operation(&plain_operation));
    Ok(operation_encoded.to_string())
}

/// Returns an encoded DELETE operation that deletes a given document.
#[wasm_bindgen(js_name = encodeDeleteOperation)]
pub fn encode_delete_operation(
    schema_id: JsValue,
    previous_operations: JsValue,
) -> Result<String, JsValue> {
    let schema_id: SchemaId = jserr!(deserialize_from_js(schema_id), "Invalid schema id");

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

    let previous_ops = jserr!(prev_op_result);
    let document_view_id = DocumentViewId::new(&previous_ops);

    // @TODO: This does not validate if the operation is correct. We can implement it as soon a we
    // use real operations with schemas again
    let plain_operation = PlainOperation::new(
        OperationVersion::V1,
        OperationAction::Update,
        schema_id,
        Some(document_view_id),
        None,
    );

    let operation_encoded = jserr!(encode_plain_operation(&plain_operation));
    Ok(operation_encoded.to_string())
}
