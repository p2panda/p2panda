// SPDX-License-Identifier: AGPL-3.0-or-later

//! Helper methods for generating common p2panda data objects. Used when generating fixtures and in the mock node and client implementations.
//! The primary reason we seperate this from the main fixture logic is that these methods can be imported and used outside of testing modules,
//! whereas the fixture macros can only be injected into rstest defined methods.
use serde::Serialize;

use crate::entry::{Entry, EntrySigned, LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::KeyPair;
use crate::operation::{
    Operation, OperationEncoded, OperationFields, OperationValue, OperationWithMeta,
};
use crate::test_utils::constants::{DEFAULT_HASH, DEFAULT_SCHEMA_HASH};

/// A custom `Result` type to be able to dynamically propagate `Error` types.
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Struct which contains the values for the next entry args needed when publishing a new entry.
#[derive(Serialize, Debug)]
pub struct NextEntryArgs {
    /// The backlink of the next entry, can be None if this is the first entry published
    pub backlink: Option<Hash>,
    /// The skiplink of the next entry, can be None if it's the same as the backlink
    pub skiplink: Option<Hash>,
    /// The seq number for the next entry
    pub seq_num: SeqNum,
    /// The log id of this log
    pub log_id: LogId,
}

/// A helper method for easily generating an operation of any type (`CREATE`, `UPDATE`, `DELETE`).
///
/// If a value for `fields` is provided, this is a `CREATE` operation.
/// If values for both `fields` and `instance_id` are provided, this is an `UPDATE` operation.
/// If no value for `fields` is provided, this is a `DELETE` operation.
pub fn any_operation(
    fields: Option<OperationFields>,
    instance_id: Option<Hash>,
    previous_operations: Option<Vec<Hash>>,
) -> Operation {
    match fields {
        // It's a CREATE operation
        Some(fields) if instance_id.is_none() => {
            Operation::new_create(hash(DEFAULT_SCHEMA_HASH), fields).unwrap()
        }
        // It's an UPDATE operation
        Some(fields) => Operation::new_update(
            hash(DEFAULT_SCHEMA_HASH),
            instance_id.unwrap(),
            previous_operations.unwrap(),
            fields,
        )
        .unwrap(),
        // It's a DELETE operation
        None => Operation::new_delete(
            hash(DEFAULT_SCHEMA_HASH),
            instance_id.unwrap(),
            previous_operations.unwrap(),
        )
        .unwrap(),
    }
}

/// Helper method for generating OperationFields from a vector of key-value tuples, currently only string types are implemented.
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

/// Generate a sequence number based on passed i64 value.
pub fn seq_num(n: i64) -> SeqNum {
    SeqNum::new(n).unwrap()
}

/// Generate a hash based on a hash string.
pub fn hash(hash_str: &str) -> Hash {
    Hash::new(hash_str).unwrap()
}

/// Generate an entry based on passed values.
pub fn entry(
    operation: Operation,
    skiplink: Option<Hash>,
    backlink: Option<Hash>,
    seq_num: SeqNum,
) -> Entry {
    Entry::new(
        &LogId::default(),
        Some(&operation),
        skiplink.as_ref(),
        backlink.as_ref(),
        &seq_num,
    )
    .unwrap()
}

/// Generate a create operation based on passed schema hash and operation fields.
pub fn create_operation(schema: Hash, fields: OperationFields) -> Operation {
    Operation::new_create(schema, fields).unwrap()
}

/// Generate an update operation based on passed schema hash, instance id and operation fields.
pub fn update_operation(
    schema: Hash,
    instance_id: Hash,
    previous_operations: Vec<Hash>,
    fields: OperationFields,
) -> Operation {
    Operation::new_update(schema, instance_id, previous_operations, fields).unwrap()
}

/// Generate a delete operation based on passed schema hash and instance id.
pub fn delete_operation(
    schema: Hash,
    instance_id: Hash,
    previous_operations: Vec<Hash>,
) -> Operation {
    Operation::new_delete(schema, instance_id, previous_operations).unwrap()
}

/// Generate a create meta operation based on passed entry hash, author and operation.
pub fn meta_operation(
    entry_signed_encoded: EntrySigned,
    operation_encoded: OperationEncoded,
) -> OperationWithMeta {
    OperationWithMeta::new(&entry_signed_encoded, &operation_encoded).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::constants::DEFAULT_HASH;

    #[test]
    fn default_hash() {
        let default_hash = Hash::new_from_bytes(vec![1, 2, 3]).unwrap();
        assert_eq!(default_hash.as_str(), DEFAULT_HASH)
    }

    #[test]
    fn default_schema() {
        let default_schema_hash = Hash::new_from_bytes(vec![3, 2, 1]).unwrap();
        assert_eq!(default_schema_hash.as_str(), DEFAULT_SCHEMA_HASH)
    }
}
