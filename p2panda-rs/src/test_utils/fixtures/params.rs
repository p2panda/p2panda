// SPDX-License-Identifier: AGPL-3.0-or-later

/// General purpose fixtures which can be injected into rstest methods as parameters.
///
/// The fixtures can optionally be passed in with custom parameters which overrides the default
/// values.
use std::convert::TryFrom;

use rand::Rng;
use rstest::fixture;

use crate::entry::{sign_and_encode, Entry, EntrySigned, SeqNum};
use crate::hash::Hash;
use crate::identity::KeyPair;
use crate::operation::{
    Operation, OperationEncoded, OperationFields, OperationValue, OperationWithMeta,
};
use crate::schema::SchemaId;
use crate::test_utils::constants::{DEFAULT_HASH, DEFAULT_PRIVATE_KEY, DEFAULT_SCHEMA_HASH};
use crate::test_utils::fixtures::defaults;
use crate::test_utils::utils;

/// Fixture which injects the default private key string into a test method.
#[fixture]
pub fn private_key() -> String {
    DEFAULT_PRIVATE_KEY.into()
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

/// Fixture which injects the default schema Hash into a test method. Default value can be
/// overridden at testing time by passing in a custom schema hash string.
#[fixture]
pub fn schema(#[default(DEFAULT_SCHEMA_HASH)] schema_str: &str) -> SchemaId {
    SchemaId::new(schema_str).unwrap()
}

/// Fixture which injects the default Hash into a test method. Default value can be overridden at
/// testing time by passing in a custom hash string.
#[fixture]
pub fn hash(#[default(DEFAULT_HASH)] hash_str: &str) -> Hash {
    utils::hash(hash_str)
}

/// Fixture which injects a random hash into a test method.
#[fixture]
pub fn random_hash() -> Hash {
    let random_data = rand::thread_rng().gen::<[u8; 32]>().to_vec();
    Hash::new_from_bytes(random_data).unwrap()
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

/// Fixture which injects the default Entry into a test method.
///
/// Default value can be overridden at testing time by passing in custom operation, seq number,
/// backlink and skiplink.
#[fixture]
pub fn entry(
    operation: Operation,
    seq_num: SeqNum,
    #[default(None)] backlink: Option<Hash>,
    #[default(None)] skiplink: Option<Hash>,
) -> Entry {
    utils::entry(operation, skiplink, backlink, seq_num)
}

/// Fixture which injects the default Operation into a test method.
///
/// Default value can be overridden at testing time by passing in custom operation fields and
/// document id.
#[fixture]
pub fn operation(
    #[from(some_fields)] fields: Option<OperationFields>,
    #[default(None)] previous_operations: Option<Vec<Hash>>,
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
pub fn entry_signed_encoded(entry: Entry, key_pair: KeyPair) -> EntrySigned {
    sign_and_encode(&entry, &key_pair).unwrap()
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
    #[default(vec![hash(DEFAULT_HASH)])] previous_operations: Vec<Hash>,
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
    #[default(vec![hash(DEFAULT_HASH)])] previous_operations: Vec<Hash>,
) -> Operation {
    utils::delete_operation(schema, previous_operations)
}

/// Fixture which injects the default CREATE OperationWithMeta into a test method.
///
/// Default value can be overridden at testing time by passing in custom schema hash and operation
/// fields.
#[fixture]
pub fn meta_operation(
    entry_signed_encoded: EntrySigned,
    operation_encoded: OperationEncoded,
) -> OperationWithMeta {
    utils::meta_operation(entry_signed_encoded, operation_encoded)
}

#[fixture]
pub fn encoded_create_string(create_operation: Operation) -> String {
    OperationEncoded::try_from(&create_operation)
        .unwrap()
        .as_str()
        .to_owned()
}
