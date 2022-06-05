use std::convert::TryFrom;

use rstest::fixture;

use crate::entry::EntrySigned;
use crate::identity::Author;
use crate::operation::{
    Operation, OperationEncoded, OperationFields, OperationId, OperationValue, OperationWithMeta,
};
use crate::schema::SchemaId;
use crate::test_utils::constants::{DEFAULT_HASH, TEST_SCHEMA_ID};
use crate::test_utils::fixtures::{entry_signed_encoded, public_key, random_hash, schema};

/// Fixture which injects the default testing `OperationId` into a test method.
///
/// Default value can be overridden at testing time by passing in a custom hash string.
#[fixture]
pub fn operation_id(#[default(DEFAULT_HASH)] hash_str: &str) -> OperationId {
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

/// Fixture which injects the default testing OperationFields value into a test method.
///
/// Default value can be overridden at testing time by passing in a custom vector of key-value
/// tuples.
#[fixture]
pub fn operation_fields(
    #[default(vec![("message", operation_value())])] fields_vec: Vec<(&str, OperationValue)>,
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
    #[default(vec![("message", operation_value())])] fields_vec: Vec<(&str, OperationValue)>,
) -> Option<OperationFields> {
    Some(operation_fields(fields_vec))
}

/// Fixture which injects the default Operation into a test method.
///
/// Default value can be overridden at testing time by passing in custom operation fields and
/// document id.
#[fixture]
pub fn operation(
    #[from(some_fields)] fields: Option<OperationFields>,
    #[default(None)] previous_operations: Option<Vec<OperationId>>,
) -> Operation {
    any_operation(fields, previous_operations)
}

/// A helper method for easily generating an operation of any type (CREATE, UPDATE or DELETE).
///
/// If a value for `fields` is provided, this is a CREATE operation. If values for both `fields`
/// and `document_id` are provided, this is an UPDATE operation. If no value for `fields` is
/// provided, this is a DELETE operation.
pub fn any_operation(
    fields: Option<OperationFields>,
    previous_operations: Option<Vec<OperationId>>,
) -> Operation {
    let schema_id = SchemaId::new(TEST_SCHEMA_ID).unwrap();
    match fields {
        // It's a CREATE operation
        Some(fields) if previous_operations.is_none() => {
            Operation::new_create(schema_id, fields).unwrap()
        }
        // It's an UPDATE operation
        Some(fields) => {
            Operation::new_update(schema_id, previous_operations.unwrap(), fields).unwrap()
        }
        // It's a DELETE operation
        None => Operation::new_delete(schema_id, previous_operations.unwrap()).unwrap(),
    }
}

/// Fixture which injects the default CREATE Operation into a test method.
///
/// Default value can be overridden at testing time by passing in custom schema hash and operation
/// fields.
#[fixture]
pub fn create_operation(
    schema: SchemaId,
    #[from(operation_fields)] fields: OperationFields,
) -> Operation {
    Operation::new_create(schema, fields).unwrap()
}

/// Fixture which injects the default UPDATE Operation into a test method.
///
/// Default value can be overridden at testing time by passing in custom schema hash, document id
/// hash and operation fields.
#[fixture]
pub fn update_operation(
    schema: SchemaId,
    #[default(vec![operation_id(DEFAULT_HASH)])] previous_operations: Vec<OperationId>,
    #[default(operation_fields(vec![("message", OperationValue::Text("Updated, hello!".to_string()))]))]
    fields: OperationFields,
) -> Operation {
    Operation::new_update(schema, previous_operations, fields).unwrap()
}

/// Fixture which injects the default DELETE Operation into a test method.
///
/// Default value can be overridden at testing time by passing in custom schema hash and document
/// id hash.
#[fixture]
pub fn delete_operation(
    schema: SchemaId,
    #[default(vec![operation_id(DEFAULT_HASH)])] previous_operations: Vec<OperationId>,
) -> Operation {
    Operation::new_delete(schema, previous_operations).unwrap()
}

/// Fixture which injects a CREATE `OperationWithMeta` into a test method.
#[fixture]
pub fn create_operation_with_meta(
    create_operation: Operation,
    public_key: Author,
    #[from(random_operation_id)] operation_id: OperationId,
) -> OperationWithMeta {
    OperationWithMeta::new_test_operation(&operation_id, &public_key, &create_operation)
}

/// Fixture which injects an UPDATE OperationWithMeta into a test method.
#[fixture]
pub fn update_operation_with_meta(
    update_operation: Operation,
    public_key: Author,
    #[from(random_operation_id)] operation_id: OperationId,
) -> OperationWithMeta {
    OperationWithMeta::new_test_operation(&operation_id, &public_key, &update_operation)
}

/// Fixture which injects a DELETE `OperationWithMeta` into a test method.
#[fixture]
pub fn delete_operation_with_meta(
    delete_operation: Operation,
    public_key: Author,
    #[from(random_operation_id)] operation_id: OperationId,
) -> OperationWithMeta {
    OperationWithMeta::new_test_operation(&operation_id, &public_key, &delete_operation)
}

#[fixture]
pub fn encoded_create_string(create_operation: Operation) -> String {
    OperationEncoded::try_from(&create_operation)
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
pub fn operation_encoded(operation: Operation) -> OperationEncoded {
    OperationEncoded::try_from(&operation).unwrap()
}
