// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;
use std::str::FromStr;

#[cfg(test)]
use serde::Deserialize;
use serde::Serialize;
use serde_bytes::ByteBuf;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;

use crate::document::DocumentViewId;
use crate::operation::plain::PlainValue;
use crate::operation::traits::{Actionable, Schematic};
use crate::operation::validate::validate_operation_format;
use crate::operation::{
    EncodedOperation, OperationAction, OperationId, OperationValue, OperationVersion,
    PinnedRelation, PinnedRelationList, Relation, RelationList,
};
use crate::schema::SchemaId;
use crate::wasm::error::jserr;
use crate::wasm::serde::{deserialize_from_js, serialize_to_js};
use crate::Validate;

/// Helper method to convert from `OperationValue` to `JsValue`.
fn operation_to_js_value(operation_value: &OperationValue) -> Result<JsValue, JsValue> {
    match operation_value {
        OperationValue::Boolean(value) => Ok(JsValue::from_bool(value.to_owned())),
        OperationValue::Bytes(value) => Ok(jserr!(serialize_to_js(&ByteBuf::from(value.to_owned())))),
        OperationValue::Integer(value) => Ok(JsValue::from(value.to_owned())),
        OperationValue::Float(value) => Ok(JsValue::from_f64(value.to_owned())),
        OperationValue::String(value) => Ok(JsValue::from_str(value)),
        OperationValue::Relation(value) => Ok(jserr!(serialize_to_js(value))),
        OperationValue::RelationList(value) => Ok(jserr!(serialize_to_js(value))),
        OperationValue::PinnedRelation(value) => Ok(jserr!(serialize_to_js(value))),
        OperationValue::PinnedRelationList(value) => Ok(jserr!(serialize_to_js(value))),
    }
}

/// Helper method to convert from `PlainValue` to `JsValue`.
fn plain_to_js_value(plain_value: &PlainValue) -> Result<JsValue, JsValue> {
    match plain_value {
        PlainValue::Boolean(value) => Ok(JsValue::from_bool(value.to_owned())),
        PlainValue::Integer(value) => Ok(JsValue::from(value.to_owned())),
        PlainValue::Float(value) => Ok(JsValue::from_f64(value.to_owned())),
        PlainValue::Bytes(value) => Ok(jserr!(serialize_to_js(&ByteBuf::from(value.to_owned())))),
        PlainValue::String(value) => Ok(JsValue::from_str(value)),
        PlainValue::AmbiguousRelation(value) => Ok(jserr!(serialize_to_js(value))),
        PlainValue::PinnedRelationList(value) => Ok(jserr!(serialize_to_js(value))),
    }
}

/// Return value of [`decode_entry`] that holds the decoded entry and plain operation.
#[derive(Serialize, Debug)]
#[cfg_attr(test, derive(Deserialize))]
#[serde(rename_all = "camelCase")]
pub struct PlainOperation {
    /// Version of this operation.
    pub(crate) version: u64,

    /// Describes if this operation creates, updates or deletes data.
    pub(crate) action: u64,

    /// The id of the schema for this operation.
    pub(crate) schema_id: String,

    /// Optional document view id containing the operation ids directly preceding this one in the
    /// document.
    pub(crate) previous: Option<Vec<String>>,

    /// Optional fields map holding the operation data.
    pub(crate) fields: Option<PlainFields>,
}

/// Interface to create, update and retreive values from operation fields.
#[wasm_bindgen]
#[derive(Clone, Serialize, Debug)]
#[cfg_attr(test, derive(Deserialize))]
pub struct PlainFields(crate::operation::plain::PlainFields);

#[wasm_bindgen]
impl PlainFields {
    /// Returns field value when existing.
    #[wasm_bindgen]
    pub fn get(&self, name: &str) -> Result<JsValue, JsValue> {
        match self.0.get(name) {
            Some(value) => Ok(jserr!(
                plain_to_js_value(value),
                "Could not retreive value from plain field"
            )),
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
}

/// Interface to create, update and retreive values from operation fields.
#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct OperationFields(crate::operation::OperationFields);

#[wasm_bindgen]
impl OperationFields {
    /// Returns an `OperationFields` instance.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self(crate::operation::OperationFields::new())
    }

    /// Adds a field with a value and a given value type.
    ///
    /// The type is defined by a simple string, similar to an enum. Possible type values are:
    ///
    /// - "bool" (Boolean)
    /// - "bytes" (Bytes)
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
                jserr!(self.0.insert(name, OperationValue::String(value_str)));
                Ok(())
            }
            "bytes" => {
                let value_bytes: ByteBuf =
                    jserr!(deserialize_from_js(value), "Expected a byte array");
                jserr!(self
                    .0
                    .insert(name, OperationValue::Bytes(value_bytes.to_vec())));
                Ok(())
            }
            "bool" => {
                let value_bool = jserr!(value.as_bool().ok_or("Invalid boolean value"));
                jserr!(self.0.insert(name, OperationValue::Boolean(value_bool)));
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
                jserr!(self.0.insert(name, OperationValue::Integer(value_int)));
                Ok(())
            }
            "float" => {
                let value_float = jserr!(value.as_f64().ok_or("Invalid float value"));
                jserr!(self.0.insert(name, OperationValue::Float(value_float)));
                Ok(())
            }
            "relation" => {
                let relation: Relation = jserr!(
                    deserialize_from_js(value),
                    "Expected a document id string for field of type relation"
                );
                jserr!(relation.validate());
                jserr!(self.0.insert(name, OperationValue::Relation(relation)));
                Ok(())
            }
            "relation_list" => {
                let relations: RelationList = jserr!(
                    deserialize_from_js(value),
                    "Expected an array of operation ids for field of type relation list"
                );
                jserr!(self.0.insert(name, OperationValue::RelationList(relations)));
                Ok(())
            }
            "pinned_relation" => {
                let operation_ids: Vec<OperationId> = jserr!(
                    deserialize_from_js(value),
                    "Expected an array of operation ids for field of type pinned relation list"
                );

                // De-duplicate and sort operation ids as the data comes via the programmatic API
                let relation = PinnedRelation::new(DocumentViewId::new(&operation_ids));

                jserr!(self
                    .0
                    .insert(name, OperationValue::PinnedRelation(relation)));
                Ok(())
            }
            "pinned_relation_list" => {
                let relations: Vec<Vec<OperationId>> = jserr!(
                    deserialize_from_js(value),
                    "Expected a nested array of operation ids for field of type pinned relation list"
                );

                // De-duplicate and sort operation ids as the data comes via the programmatic API
                let document_view_ids: Vec<DocumentViewId> = relations
                    .iter()
                    .map(|operation_ids| DocumentViewId::new(&operation_ids))
                    .collect();
                let relations = PinnedRelationList::new(document_view_ids);

                jserr!(self
                    .0
                    .insert(name, OperationValue::PinnedRelationList(relations)));
                Ok(())
            }
            _ => Err(js_sys::Error::new("Unknown value type").into()),
        }
    }

    /// Returns field of this `OperationFields` instance when existing.
    #[wasm_bindgen]
    pub fn get(&self, name: &str) -> Result<JsValue, JsValue> {
        match self.0.get(name) {
            Some(value) => Ok(jserr!(
                operation_to_js_value(value),
                "Could not retreive value from operation field"
            )),
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
}

impl Default for OperationFields {
    fn default() -> Self {
        Self::new()
    }
}

/// Creates, validates and encodes an operation as hexadecimal string.
#[wasm_bindgen(js_name = encodeOperation)]
pub fn encode_operation(
    action: u64,
    schema_id: String,
    previous: JsValue,
    fields: Option<OperationFields>,
) -> Result<String, JsValue> {
    // Convert parameters
    let action = jserr!(OperationAction::try_from(action));
    let schema_id = jserr!(SchemaId::from_str(&schema_id));
    let document_view_id = if !previous.is_undefined() {
        let view_id: DocumentViewId =
            jserr!(deserialize_from_js(previous), "Invalid document view id");
        Some(view_id)
    } else {
        None
    };

    // Create and validate operation
    let operation = crate::operation::Operation {
        version: OperationVersion::V1,
        action,
        schema_id,
        previous: document_view_id,
        fields: fields.map(|inner| inner.0),
    };
    jserr!(validate_operation_format(&operation));

    // Encode and return it as hexadecimal string
    let operation_encoded = jserr!(crate::operation::encode::encode_operation(&operation));
    Ok(operation_encoded.to_string())
}

/// Decodes an operation into its plain form.
///
/// A plain operation has not been checked against a schema yet.
#[wasm_bindgen(js_name = decodeOperation)]
pub fn decode_operation(encoded_operation: String) -> Result<JsValue, JsValue> {
    let operation_bytes = jserr!(
        hex::decode(encoded_operation),
        "Invalid hex-encoding of encoded operation"
    );
    let operation_encoded = EncodedOperation::from_bytes(&operation_bytes);

    // Decode to plain operation
    let operation_plain = jserr!(crate::operation::decode::decode_operation(
        &operation_encoded
    ));

    // Convert document view id into array of operation id strings
    let previous: Option<Vec<String>> = match operation_plain.previous() {
        Some(previous) => {
            let converted: Vec<String> = previous
                .graph_tips()
                .iter()
                .map(|operation_id| operation_id.to_string())
                .collect();

            Some(converted)
        }
        None => None,
    };

    // Convert plain fields into map of js values
    let fields = match operation_plain.fields() {
        Some(inner) => Some(PlainFields(inner)),
        None => None,
    };

    // Convert to external wasm type and return it
    let result_wasm = PlainOperation {
        action: operation_plain.action().as_u64(),
        version: operation_plain.version().as_u64(),
        schema_id: operation_plain.schema_id().to_string(),
        previous,
        fields,
    };
    let result = jserr!(serialize_to_js(&result_wasm));
    Ok(result)
}
