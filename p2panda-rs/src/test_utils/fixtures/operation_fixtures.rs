// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;

use rstest::fixture;

use crate::document::DocumentViewId;
use crate::entry::EntrySigned;
use crate::identity::Author;
use crate::operation::{
    Operation, OperationEncoded, OperationFields, OperationId, OperationValue, OperationWithMeta,
};
use crate::schema::SchemaId;
use crate::test_utils::constants::{self, TEST_SCHEMA_ID};
use crate::test_utils::fixtures::{entry_signed_encoded, public_key, random_hash, schema};

/// Fixture which injects the default testing `OperationId` into a test method.
///
/// Default value can be overridden at testing time by passing in a custom hash string.
#[fixture]
pub fn operation_id(#[default(constants::DEFAULT_HASH)] hash_str: &str) -> OperationId {
    hash_str.parse().unwrap()
}

/// Fixture which injects a random operation id into a test method.
#[fixture]
pub fn random_operation_id() -> OperationId {
    random_hash().into()
}

#[fixture]
pub fn operation_value() -> OperationValue {
    OperationValue::Text("Hello!".to_string())
}

#[fixture]
pub fn random_previous_operations(#[default(1)] num: u32) -> DocumentViewId {
    let mut previous_operations: Vec<OperationId> = Vec::new();
    for _ in 0..num {
        previous_operations.push(random_hash().into())
    }
    DocumentViewId::new(&previous_operations).unwrap()
}

/// Fixture which injects the default testing OperationFields value into a test method.
///
/// Default value can be overridden at testing time by passing in a custom vector of key-value
/// tuples.
#[fixture]
pub fn operation_fields(
    #[default(constants::default_fields())] fields_vec: Vec<(&str, OperationValue)>,
) -> OperationFields {
    let mut operation_fields = OperationFields::new();
    for (key, value) in fields_vec.iter() {
        operation_fields.add(key, value.to_owned()).unwrap();
    }
    operation_fields
}

/// Fixture which injects the default OperationFields value into a test method wrapped in an option.
///
/// Default value can be overridden at testing time by passing in a custom vector of key-value
/// tuples.
#[fixture]
pub fn some_fields(
    #[default(constants::default_fields())] fields_vec: Vec<(&str, OperationValue)>,
) -> Option<OperationFields> {
    Some(operation_fields(fields_vec))
}

/// Fixture which injects the default Operation into a test method.
///
/// Default value can be overridden at testing time by passing in custom operation fields and
/// document id.
///
/// If a value for `fields` is provided, this is a CREATE operation. If values for both `fields`
/// and `document_id` are provided, this is an UPDATE operation. If no value for `fields` is
/// provided, this is a DELETE operation.
#[fixture]
pub fn operation(
    #[from(some_fields)] fields: Option<OperationFields>,
    #[default(None)] previous_operations: Option<DocumentViewId>,
    #[default(Some(TEST_SCHEMA_ID.parse().unwrap()))] schema: Option<SchemaId>,
) -> Operation {
    let schema = schema.unwrap_or(TEST_SCHEMA_ID.parse().unwrap());
    match fields {
        // It's a CREATE operation
        Some(fields) if previous_operations.is_none() => {
            Operation::new_create(schema, fields).unwrap()
        }
        // It's an UPDATE operation
        Some(fields) => {
            Operation::new_update(schema, previous_operations.unwrap(), fields).unwrap()
        }
        // It's a DELETE operation
        None => Operation::new_delete(schema, previous_operations.unwrap()).unwrap(),
    }
}

/// Fixture which injects a test `OperationWithMeta` into a test method.
#[fixture]
pub fn operation_with_meta(
    #[from(some_fields)] fields: Option<OperationFields>,
    #[default(None)] previous_operations: Option<DocumentViewId>,
    #[default(Some(TEST_SCHEMA_ID.parse().unwrap()))] schema: Option<SchemaId>,
    #[default(Some(public_key()))] author: Option<Author>,
    #[default(Some(random_operation_id()))] operation_id: Option<OperationId>,
) -> OperationWithMeta {
    OperationWithMeta::new_test_operation(
        &operation_id.unwrap_or(random_operation_id()),
        &author.unwrap_or(public_key()),
        &operation(fields, previous_operations, schema),
    )
}

#[fixture]
pub fn encoded_create_string(operation: Operation) -> String {
    OperationEncoded::try_from(&operation)
        .unwrap()
        .as_str()
        .to_owned()
}

/// Fixture which injects the default CREATE `OperationWithMeta` into a test method.
#[fixture]
pub fn meta_operation(
    entry_signed_encoded: EntrySigned,
    operation_encoded: OperationEncoded,
) -> OperationWithMeta {
    OperationWithMeta::new_from_entry(&entry_signed_encoded, &operation_encoded).unwrap()
}

#[fixture]
pub fn operation_encoded(
    #[from(some_fields)] fields: Option<OperationFields>,
    #[default(None)] previous_operations: Option<DocumentViewId>,
    #[default(Some(TEST_SCHEMA_ID.parse().unwrap()))] schema: Option<SchemaId>,
) -> OperationEncoded {
    OperationEncoded::try_from(&operation(fields, previous_operations, schema)).unwrap()
}

/// Invalid YASMF hash in `document` with correct length but unknown hash format identifier.
#[fixture]
pub fn operation_encoded_invalid_relation_fields() -> OperationEncoded {
    // {
    //   "action": "create",
    //   "schema": "venue_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b",
    //   "version": 1,
    //   "fields": {
    //     "locations": {
    //       "type": "relation",
    //       "value": "83e2043738f2b5cdcd3b6cb0fbb82fe125905d0f75e16488a38d395ff5f9d5ea82b5"
    //     }
    //   }
    // }
    OperationEncoded::new("A466616374696F6E6663726561746566736368656D61784A76656E75655F30303230633635353637616533376566656132393365333461396337643133663866326266323364626463336235633762396162343632393331313163343866633738626776657273696F6E01666669656C6473A1696C6F636174696F6E73A264747970656872656C6174696F6E6576616C756578443833653230343337333866326235636463643362366362306662623832666531323539303564306637356531363438386133386433393566663566396435656138326235").unwrap()
}

// Helpers for easily constructing common operation types

pub fn create_operation(fields: &[(&str, OperationValue)]) -> Operation {
    operation(Some(operation_fields(fields.to_vec())), None, None)
}

pub fn update_operation(
    fields: &[(&str, OperationValue)],
    previous_operations: &DocumentViewId,
) -> Operation {
    operation(
        Some(operation_fields(fields.to_vec())),
        Some(previous_operations.clone()),
        None,
    )
}

pub fn delete_operation(previous_operations: &DocumentViewId) -> Operation {
    operation(None, Some(previous_operations.to_owned()), None)
}
