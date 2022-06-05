// SPDX-License-Identifier: AGPL-3.0-or-later

/// General purpose fixtures which can be injected into rstest methods as parameters.
///
/// The fixtures can optionally be passed in with custom parameters which overrides the default
/// values.
use std::convert::TryFrom;

use rand::Rng;
use rstest::fixture;

use crate::document::{DocumentId, DocumentViewId};
use crate::entry::{sign_and_encode, Entry, EntrySigned, SeqNum};
use crate::hash::Hash;
use crate::identity::{Author, KeyPair};
use crate::operation::{
    Operation, OperationEncoded, OperationFields, OperationId, OperationValue, OperationWithMeta,
};
use crate::schema::SchemaId;
use crate::test_utils::constants::{DEFAULT_HASH, DEFAULT_PRIVATE_KEY, TEST_SCHEMA_ID};
use crate::test_utils::fixtures::defaults;
use crate::test_utils::utils;

use crate::test_utils::fixtures::*;

/// Fixture which injects the default private key string into a test method.
#[fixture]
pub fn private_key() -> String {
    DEFAULT_PRIVATE_KEY.into()
}

/// Fixture which injects the default author into a test method.
#[fixture]
pub fn public_key() -> Author {
    let key_pair = KeyPair::from_private_key_str(DEFAULT_PRIVATE_KEY).unwrap();
    Author::try_from(key_pair.public_key().to_owned()).unwrap()
}

/// Fixture which injects the default KeyPair into a test method. Default value can be overridden
/// at testing time by passing in a custom private key string.
#[fixture]
pub fn key_pair(#[default(DEFAULT_PRIVATE_KEY.into())] private_key: String) -> KeyPair {
    utils::keypair_from_private(private_key)
}

/// Fixture which injects a random KeyPair into a test method.
#[fixture]
pub fn random_key_pair() -> KeyPair {
    utils::new_key_pair()
}

/// Fixture which injects the default SeqNum into a test method. Default value can be overridden at
/// testing time by passing in a custom seq num as u64.
#[fixture]
pub fn seq_num(#[default(1)] n: u64) -> SeqNum {
    utils::seq_num(n)
}

/// Fixture which injects the default schema id into a test method. Default value can be
/// overridden at testing time by passing in a custom schema id string.
#[fixture]
pub fn schema(#[default(TEST_SCHEMA_ID)] schema_id: &str) -> SchemaId {
    SchemaId::new(schema_id).unwrap()
}

/// Fixture which injects the default Hash into a test method. Default value can be overridden at
/// testing time by passing in a custom hash string.
#[fixture]
pub fn hash(#[default(DEFAULT_HASH)] hash_str: &str) -> Hash {
    utils::hash(hash_str)
}

/// Fixture which injects the default `DocumentId` into a test method. Default value can be overridden at
/// testing time by passing in a custom hash string.
#[fixture]
pub fn document_id(#[default(DEFAULT_HASH)] hash_str: &str) -> DocumentId {
    DocumentId::new(operation_id(hash_str))
}

/// Fixture which injects the default `DocumentViewId` into a test method. Default value can be
/// overridden at testing time by passing in a custom hash string vector.
#[fixture]
pub fn document_view_id(#[default(vec![DEFAULT_HASH])] hash_str_vec: Vec<&str>) -> DocumentViewId {
    let hashes: Vec<OperationId> = hash_str_vec
        .into_iter()
        .map(|hash| hash.parse::<OperationId>().unwrap())
        .collect();
    DocumentViewId::new(&hashes).unwrap()
}

/// Fixture which injects the default `OperationId` into a test method. Default value can be
/// overridden at testing time by passing in a custom hash string.
#[fixture]
pub fn operation_id(#[default(DEFAULT_HASH)] hash_str: &str) -> OperationId {
    OperationId::new(hash(hash_str))
}

/// Fixture which injects a random hash into a test method.
#[fixture]
pub fn random_hash() -> Hash {
    let random_data = rand::thread_rng().gen::<[u8; 32]>().to_vec();
    Hash::new_from_bytes(random_data).unwrap()
}

/// Fixture which injects a random operation id into a test method.
#[fixture]
pub fn random_operation_id() -> OperationId {
    random_hash().into()
}

/// Fixture which injects a random document id.
#[fixture]
pub fn random_document_id() -> DocumentId {
    DocumentId::new(random_hash().into())
}

/// Fixture which injects a random document view id into a test method.
#[fixture]
pub fn random_document_view_id() -> DocumentViewId {
    DocumentViewId::new(&[random_hash().into(), random_hash().into()]).unwrap()
}

/// Fixture which injects the default OperationFields value into a test method.
///
/// Default value can be overridden at testing time by passing in a custom vector of key-value
/// tuples.
#[fixture]
pub fn fields(
    #[default(vec![("message", defaults::operation_value())])] fields_vec: Vec<(
        &str,
        OperationValue,
    )>,
) -> OperationFields {
    utils::operation_fields(fields_vec)
}

/// Fixture which injects the default OperationFields value into a test method.
///
/// Default value can be overridden at testing time by passing in a custom vector of key-value
/// tuples.
#[fixture]
pub fn some_fields(
    #[default(vec![("message", defaults::operation_value())])] fields_vec: Vec<(
        &str,
        OperationValue,
    )>,
) -> Option<OperationFields> {
    Some(utils::operation_fields(fields_vec))
}

/// Fixture which injects the default Operation into a test method.
///
/// Default value can be overridden at testing time by passing in custom operation fields and
/// document id.
#[fixture]
pub fn operation(
    #[from(some_fields)] fields: Option<OperationFields>,
    #[default(None)] previous_operations: Option<DocumentViewId>,
) -> Operation {
    utils::any_operation(fields, previous_operations)
}

/// Fixture which injects the default Hash into a test method as an Option.
///
/// Default value can be overridden at testing time by passing in custom hash string.
#[fixture]
pub fn some_hash(#[default(DEFAULT_HASH)] str: &str) -> Option<Hash> {
    let hash = Hash::new(str);
    Some(hash.unwrap())
}

#[fixture]
pub fn operation_encoded(operation: Operation) -> OperationEncoded {
    OperationEncoded::try_from(&operation).unwrap()
}

/// Fixture which injects the default CREATE Operation into a test method.
///
/// Default value can be overridden at testing time by passing in custom schema hash and operation
/// fields.
#[fixture]
pub fn create_operation(schema: SchemaId, fields: OperationFields) -> Operation {
    utils::create_operation(schema, fields)
}

/// Fixture which injects the default UPDATE Operation into a test method.
///
/// Default value can be overridden at testing time by passing in custom schema hash, document id
/// hash and operation fields.
#[fixture]
pub fn update_operation(
    schema: SchemaId,
    #[default(document_view_id(vec![DEFAULT_HASH]))] previous_operations: DocumentViewId,
    #[default(fields(vec![("message", OperationValue::Text("Updated, hello!".to_string()))]))]
    fields: OperationFields,
) -> Operation {
    utils::update_operation(schema, previous_operations, fields)
}

/// Fixture which injects the default DELETE Operation into a test method.
///
/// Default value can be overridden at testing time by passing in custom schema hash and document
/// id hash.
#[fixture]
pub fn delete_operation(
    schema: SchemaId,
    #[default(document_view_id(vec![DEFAULT_HASH]))] previous_operations: DocumentViewId,
) -> Operation {
    utils::delete_operation(schema, previous_operations)
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
    utils::meta_operation(entry_signed_encoded, operation_encoded)
}
