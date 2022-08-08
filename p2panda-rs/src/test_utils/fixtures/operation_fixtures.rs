// SPDX-License-Identifier: AGPL-3.0-or-later

use rstest::fixture;

use crate::document::DocumentViewId;
use crate::entry::encode::{encode_entry, sign_entry};
use crate::entry::{LogId, SeqNum};
use crate::identity::KeyPair;
use crate::operation::encode::{encode_operation, encode_plain_operation};
use crate::operation::plain::PlainOperation;
use crate::operation::validate::validate_operation_with_entry;
use crate::operation::{
    EncodedOperation, Operation, OperationAction, OperationFields, OperationId, OperationValue,
    OperationVersion, VerifiedOperation,
};
use crate::schema::Schema;
use crate::test_utils::constants::{test_fields, HASH, SCHEMA_ID};
use crate::test_utils::fixtures::{
    document_view_id, key_pair, random_hash, schema, schema_fields, schema_id,
};

/// Returns constant testing operation id.
#[fixture]
pub fn operation_id(#[default(HASH)] hash_str: &str) -> OperationId {
    hash_str.parse().unwrap()
}

/// Generates a new random operation id.
#[fixture]
pub fn random_operation_id() -> OperationId {
    random_hash().into()
}

/// Returns constant operation value.
#[fixture]
pub fn operation_value() -> OperationValue {
    OperationValue::String("Hello!".to_string())
}

/// Returns document view id of any number of operations containing random hashes.
#[fixture]
pub fn random_previous_operations(#[default(1)] num: u32) -> DocumentViewId {
    let mut previous_operations: Vec<OperationId> = Vec::new();

    for _ in 0..num {
        previous_operations.push(random_hash().into())
    }

    // Make sure the random hashes are sorted, otherwise validation will fail when creating the
    // document view id
    previous_operations.sort();

    DocumentViewId::new(&previous_operations)
}

/// Returns operation fields populated with test values.
#[fixture]
pub fn operation_fields(
    #[default(test_fields())] fields_vec: Vec<(&str, OperationValue)>,
) -> OperationFields {
    let mut operation_fields = OperationFields::new();
    for (key, value) in fields_vec.iter() {
        if let Err(_) = operation_fields.insert(key, value.to_owned()) {
            // Ignore duplicates error
        }
    }
    operation_fields
}

/// Returns operation fields wrapped in an option.
#[fixture]
pub fn some_fields(
    #[default(test_fields())] fields_vec: Vec<(&str, OperationValue)>,
) -> Option<OperationFields> {
    Some(operation_fields(fields_vec))
}

/// Returns an operation.
///
/// If a value for `fields` is provided, this is a CREATE operation. If values for both `fields`
/// and `previous_operations` are provided, this is an UPDATE operation. If no value for `fields`
/// is provided, this is a DELETE operation.
#[fixture]
pub fn operation(
    #[from(some_fields)] fields: Option<OperationFields>,
    #[default(None)] previous_operations: Option<DocumentViewId>,
    #[from(schema)] schema: Schema,
) -> Operation {
    match fields {
        // It's a CREATE operation
        Some(fields) if previous_operations.is_none() => Operation {
            version: OperationVersion::V1,
            action: OperationAction::Create,
            schema,
            previous_operations: None,
            fields: Some(fields),
        },
        // It's an UPDATE operation
        Some(fields) => Operation {
            version: OperationVersion::V1,
            action: OperationAction::Update,
            schema,
            previous_operations,
            fields: Some(fields),
        },
        // It's a DELETE operation
        None => Operation {
            version: OperationVersion::V1,
            action: OperationAction::Delete,
            schema,
            previous_operations,
            fields: None,
        },
    }
}

/// Returns an CREATE operation with a constant testing schema.
#[fixture]
pub fn operation_with_schema(
    #[from(some_fields)] fields: Option<OperationFields>,
    #[default(None)] previous_operations: Option<DocumentViewId>,
) -> Operation {
    let schema_id = schema_id(SCHEMA_ID);

    let schema = schema(
        schema_fields(test_fields(), schema_id.clone()),
        schema_id,
        "Test schema",
    );

    operation(fields, previous_operations, schema)
}

/// Returns an constant CREATE operation with a constant testing schema.
#[fixture]
pub fn create_operation_with_schema() -> Operation {
    let schema_id = schema_id(SCHEMA_ID);

    let schema = schema(
        schema_fields(test_fields(), schema_id.clone()),
        schema_id,
        "Test schema",
    );

    operation(some_fields(test_fields()), None, schema)
}

/// Generates verified operation instance.
///
/// If a value for `fields` is provided, this is a CREATE operation. If values for both `fields`
/// and `previous_operations` are provided, this is an UPDATE operation. If no value for `fields`
/// is provided, this is a DELETE operation.
#[fixture]
pub fn verified_operation(
    #[from(some_fields)] fields: Option<OperationFields>,
    #[from(schema)] schema: Schema,
    #[default(None)] previous_operations: Option<DocumentViewId>,
    #[from(key_pair)] key_pair: KeyPair,
) -> VerifiedOperation {
    let operation = operation(fields, previous_operations, schema.clone());
    let operation_plain: PlainOperation = (&operation).into();
    let operation_encoded = encode_plain_operation(&operation_plain).unwrap();

    let entry = sign_entry(
        &LogId::default(),
        &SeqNum::default(),
        None,
        None,
        &operation_encoded,
        &key_pair,
    )
    .unwrap();

    let entry_encoded = encode_entry(&entry).unwrap();

    validate_operation_with_entry(
        &entry,
        &entry_encoded,
        None,
        None,
        None,
        None,
        &operation_plain,
        &operation_encoded,
        &schema,
    )
    .unwrap()
}

/// Generates verified operation instance with a constant schema.
#[fixture]
pub fn verified_operation_with_schema(
    #[from(some_fields)] fields: Option<OperationFields>,
    #[default(None)] previous_operations: Option<DocumentViewId>,
    #[from(key_pair)] key_pair: KeyPair,
) -> VerifiedOperation {
    let schema_id = schema_id(SCHEMA_ID);

    let schema = schema(
        schema_fields(test_fields(), schema_id.clone()),
        schema_id,
        "Test schema",
    );

    verified_operation(fields, schema, previous_operations, key_pair)
}

/// Returns encoded operation as hexadecimal string.
#[fixture]
pub fn encoded_create_string(operation: Operation) -> String {
    let operation_encoded = encode_operation(&operation).unwrap();
    operation_encoded.to_string()
}

/// Returns encoded operation.
#[fixture]
pub fn encoded_operation(
    #[from(some_fields)] fields: Option<OperationFields>,
    #[default(None)] previous_operations: Option<DocumentViewId>,
    #[from(schema)] schema: Schema,
) -> EncodedOperation {
    let operation = operation(fields, previous_operations, schema);
    encode_operation(&operation).unwrap()
}

/// Helper method for easily constructing a CREATE operation.
#[fixture]
pub fn create_operation(
    #[default(test_fields())] fields: Vec<(&str, OperationValue)>,
    #[from(schema)] schema: Schema,
) -> Operation {
    operation(Some(operation_fields(fields.to_vec())), None, schema)
}

/// Helper method for easily constructing an UPDATE operation.
#[fixture]
pub fn update_operation(
    #[default(test_fields())] fields: Vec<(&str, OperationValue)>,
    #[from(document_view_id)] previous_operations: DocumentViewId,
    #[from(schema)] schema: Schema,
) -> Operation {
    operation(
        Some(operation_fields(fields.to_vec())),
        Some(previous_operations.clone()),
        schema,
    )
}

/// Helper method for easily constructing a DELETE operation.
#[fixture]
pub fn delete_operation(
    #[from(document_view_id)] previous_operations: DocumentViewId,
    #[from(schema)] schema: Schema,
) -> Operation {
    operation(None, Some(previous_operations.to_owned()), schema)
}
