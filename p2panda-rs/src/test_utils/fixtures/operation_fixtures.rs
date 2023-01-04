// SPDX-License-Identifier: AGPL-3.0-or-later

use rstest::fixture;

use crate::document::{DocumentId, DocumentViewId};
use crate::entry::encode::{encode_entry, sign_entry};
use crate::entry::{LogId, SeqNum};
use crate::identity::KeyPair;
use crate::operation::encode::{encode_operation, encode_plain_operation};
use crate::operation::plain::PlainOperation;
use crate::operation::traits::AsOperation;
use crate::operation::validate::validate_operation_with_entry;
use crate::operation::{
    EncodedOperation, Operation, OperationAction, OperationFields, OperationId, OperationValue,
    OperationVersion,
};
use crate::schema::{Schema, SchemaId};
use crate::test_utils::constants::{test_fields, HASH, SCHEMA_ID};
use crate::test_utils::memory_store::PublishedOperation;
use crate::test_utils::fixtures::{
    document_view_id, key_pair, random_document_id, random_hash, schema, schema_fields, schema_id,
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
    let mut previous: Vec<OperationId> = Vec::new();

    for _ in 0..num {
        previous.push(random_hash().into())
    }

    // Make sure the random hashes are sorted, otherwise validation will fail when creating the
    // document view id
    previous.sort();

    DocumentViewId::new(&previous)
}

/// Returns operation fields populated with test values.
#[fixture]
pub fn operation_fields(
    #[default(test_fields())] fields_vec: Vec<(&str, OperationValue)>,
) -> OperationFields {
    let mut operation_fields = OperationFields::new();
    for (key, value) in fields_vec.iter() {
        if operation_fields.insert(key, value.to_owned()).is_err() {
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
/// and `previous` are provided, this is an UPDATE operation. If no value for `fields`
/// is provided, this is a DELETE operation.
#[fixture]
pub fn operation(
    #[from(some_fields)] fields: Option<OperationFields>,
    #[default(None)] previous: Option<DocumentViewId>,
    #[from(schema_id)] schema_id: SchemaId,
) -> Operation {
    match fields {
        // It's a CREATE operation
        Some(fields) if previous.is_none() => Operation {
            version: OperationVersion::V1,
            action: OperationAction::Create,
            schema_id,
            previous: None,
            fields: Some(fields),
        },
        // It's an UPDATE operation
        Some(fields) => Operation {
            version: OperationVersion::V1,
            action: OperationAction::Update,
            schema_id,
            previous,
            fields: Some(fields),
        },
        // It's a DELETE operation
        None => Operation {
            version: OperationVersion::V1,
            action: OperationAction::Delete,
            schema_id,
            previous,
            fields: None,
        },
    }
}

/// Returns an CREATE operation with a constant testing schema id.
#[fixture]
pub fn operation_with_schema(
    #[from(some_fields)] fields: Option<OperationFields>,
    #[default(None)] previous: Option<DocumentViewId>,
) -> Operation {
    let schema_id = schema_id(SCHEMA_ID);

    operation(fields, previous, schema_id)
}

/// Returns an constant CREATE operation with a constant testing schema id.
#[fixture]
pub fn create_operation_with_schema() -> Operation {
    let schema_id = schema_id(SCHEMA_ID);

    operation(some_fields(test_fields()), None, schema_id)
}

/// Returns an constant encoded CREATE operation with a constant testing schema id.
#[fixture]
pub fn encoded_create_operation_with_schema() -> EncodedOperation {
    let schema_id = schema_id(SCHEMA_ID);

    encode_operation(&operation(some_fields(test_fields()), None, schema_id)).unwrap()
}

/// Generates verified operation instance.
///
/// If a value for `fields` is provided, this is a CREATE operation. If values for both `fields`
/// and `previous` are provided, this is an UPDATE operation. If no value for `fields`
/// is provided, this is a DELETE operation.
#[fixture]
pub fn published_operation(
    #[from(some_fields)] fields: Option<OperationFields>,
    #[from(schema)] schema: Schema,
    #[default(None)] previous: Option<DocumentViewId>,
    #[from(key_pair)] key_pair: KeyPair,
) -> PublishedOperation {
    let operation = operation(fields, previous, schema.id().clone());
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

    let (operation, id) = validate_operation_with_entry(
        &entry,
        &entry_encoded,
        None,
        None,
        &operation_plain,
        &operation_encoded,
        &schema,
    )
    .unwrap();

    let document_id = if operation.is_create() {
        DocumentId::new(&id)
    } else {
        random_document_id()
    };

    PublishedOperation(id, operation, key_pair.public_key(), document_id)
}

/// Generates verified operation instance with a constant schema.
#[fixture]
pub fn published_operation_with_schema(
    #[from(some_fields)] fields: Option<OperationFields>,
    #[default(None)] previous: Option<DocumentViewId>,
    #[from(key_pair)] key_pair: KeyPair,
) -> PublishedOperation {
    let schema_id = schema_id(SCHEMA_ID);

    let schema = schema(
        schema_fields(test_fields(), schema_id.clone()),
        schema_id,
        "Test schema",
    );

    published_operation(fields, schema, previous, key_pair)
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
    #[default(None)] previous: Option<DocumentViewId>,
    #[from(schema_id)] schema_id: SchemaId,
) -> EncodedOperation {
    let operation = operation(fields, previous, schema_id);
    encode_operation(&operation).unwrap()
}

/// Helper method for easily constructing a CREATE operation.
#[fixture]
pub fn create_operation(
    #[default(test_fields())] fields: Vec<(&str, OperationValue)>,
    #[from(schema_id)] schema_id: SchemaId,
) -> Operation {
    operation(Some(operation_fields(fields.to_vec())), None, schema_id)
}

/// Helper method for easily constructing an UPDATE operation.
#[fixture]
pub fn update_operation(
    #[default(test_fields())] fields: Vec<(&str, OperationValue)>,
    #[from(document_view_id)] previous: DocumentViewId,
    #[from(schema_id)] schema_id: SchemaId,
) -> Operation {
    operation(
        Some(operation_fields(fields.to_vec())),
        Some(previous),
        schema_id,
    )
}

/// Helper method for easily constructing a DELETE operation.
#[fixture]
pub fn delete_operation(
    #[from(document_view_id)] previous: DocumentViewId,
    #[from(schema_id)] schema_id: SchemaId,
) -> Operation {
    operation(None, Some(previous), schema_id)
}
