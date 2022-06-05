// SPDX-License-Identifier: AGPL-3.0-or-later

//! Helper methods for generating common p2panda data objects.
//!
//! Used when generating fixtures and in the mock node and client implementations.
//!
//! The primary reason we separate this from the main fixture logic is that these methods can be
//! imported and used outside of testing modules, whereas the fixture macros can only be injected
//! into `rstest` defined methods.
use serde::Serialize;

use crate::document::DocumentViewId;
use crate::entry::{Entry, EntrySigned, LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::KeyPair;
use crate::operation::{
    Operation, OperationEncoded, OperationFields, OperationValue, OperationWithMeta,
};
use crate::schema::SchemaId;
use crate::test_utils::constants::TEST_SCHEMA_ID;

/// A custom `Result` type to be able to dynamically propagate `Error` types.
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Struct which contains the values for the next entry args needed when publishing a new entry.
#[derive(Serialize, Debug)]
pub struct NextEntryArgs {
    /// The backlink of the next entry, can be None if this is the first entry published.
    pub backlink: Option<Hash>,

    /// The skiplink of the next entry, can be None if it's the same as the backlink.
    pub skiplink: Option<Hash>,

    /// The seq number for the next entry.
    pub seq_num: SeqNum,

    /// The log id of this log.
    pub log_id: LogId,
}

/// A helper method for easily generating an operation of any type (CREATE, UPDATE or DELETE).
///
/// If a value for `fields` is provided, this is a CREATE operation. If values for both `fields`
/// and `document_id` are provided, this is an UPDATE operation. If no value for `fields` is
/// provided, this is a DELETE operation.
pub fn any_operation(
    fields: Option<OperationFields>,
    previous_operations: Option<DocumentViewId>,
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

/// Helper method for generating OperationFields from a vector of key-value tuples, currently only
/// string types are implemented.
pub fn operation_fields(fields: Vec<(&str, OperationValue)>) -> OperationFields {
    let mut operation_fields = OperationFields::new();
    for (key, value) in fields.iter() {
        operation_fields.add(key, value.to_owned()).unwrap();
    }
    operation_fields
}

/// Generate a new key pair, not based on the default private key.
pub fn new_key_pair() -> KeyPair {
    KeyPair::new()
}

/// Generate a key pair from a private key.
pub fn keypair_from_private(private_key: String) -> KeyPair {
    KeyPair::from_private_key_str(&private_key).unwrap()
}

/// Generate a hash based on a hash string.
pub fn hash(hash_str: &str) -> Hash {
    Hash::new(hash_str).unwrap()
}

/// Generate an application schema based on a schema id string.
pub fn schema(schema_id: &str) -> SchemaId {
    SchemaId::new(schema_id).unwrap()
}

/// Generate a CREATE operation based on passed schema id and operation fields.
pub fn create_operation(schema: SchemaId, fields: OperationFields) -> Operation {
    Operation::new_create(schema, fields).unwrap()
}

/// Generate an UPDATE operation based on passed schema id, document id and operation fields.
pub fn update_operation(
    schema: SchemaId,
    previous_operations: DocumentViewId,
    fields: OperationFields,
) -> Operation {
    Operation::new_update(schema, previous_operations, fields).unwrap()
}

/// Generate a DELETE operation based on passed schema id and document id.
pub fn delete_operation(schema: SchemaId, previous_operations: DocumentViewId) -> Operation {
    Operation::new_delete(schema, previous_operations).unwrap()
}

/// Generate a CREATE meta-operation based on passed encoded entry and operation.
pub fn meta_operation(
    entry_signed_encoded: EntrySigned,
    operation_encoded: OperationEncoded,
) -> OperationWithMeta {
    OperationWithMeta::new_from_entry(&entry_signed_encoded, &operation_encoded).unwrap()
}
