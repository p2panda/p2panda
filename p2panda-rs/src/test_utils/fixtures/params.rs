// SPDX-License-Identifier: AGPL-3.0-or-later

/// General purpose fixtures which can be injected into rstest methods as parameters.
///
/// The fixtures can optionally be passed in with custom parameters which overrides the default
/// values.
use std::convert::TryFrom;

use rand::Rng;
use rstest::{fixture, rstest};
use std::sync::{Arc, Mutex};

use crate::document::{DocumentId, DocumentViewId};
use crate::entry::{sign_and_encode, Entry, EntrySigned, LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::{Author, KeyPair};
use crate::operation::{
    Operation, OperationEncoded, OperationFields, OperationId, OperationValue, OperationWithMeta,
};
use crate::schema::SchemaId;
use crate::storage_provider::entry::AsStorageEntry;
use crate::storage_provider::log::AsStorageLog;
use crate::storage_provider::{SimplestStorageProvider, StorageEntry, StorageLog};
use crate::test_utils::constants::{DEFAULT_HASH, DEFAULT_PRIVATE_KEY, TEST_SCHEMA_ID};
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
    DocumentViewId::new(&hashes)
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
    #[default(None)] previous_operations: Option<Vec<OperationId>>,
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
    #[default(vec![operation_id(DEFAULT_HASH)])] previous_operations: Vec<OperationId>,
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
    #[default(vec![operation_id(DEFAULT_HASH)])] previous_operations: Vec<OperationId>,
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

pub const SKIPLINK_ENTRIES: [u64; 2] = [4, 8];

#[fixture]
pub fn test_db(
    #[from(random_key_pair)] key_pair: KeyPair,
    create_operation: Operation,
    fields: OperationFields,
    schema: SchemaId,
    document_id: DocumentId,
) -> SimplestStorageProvider {
    // Initial empty entry vec.
    let mut db_entries: Vec<StorageEntry> = vec![];

    // Create a log vec with one log in it (which we create the entries for below)
    let author = Author::try_from(key_pair.public_key().to_owned()).unwrap();
    let db_logs: Vec<StorageLog> = vec![StorageLog::new(
        &author,
        &schema,
        &document_id,
        &LogId::new(1),
    )];

    // Create and push a first entry containing a CREATE operation to the entries list
    let create_entry = entry(
        create_operation.clone(),
        SeqNum::new(1).unwrap(),
        None,
        None,
    );

    let encoded_entry = sign_and_encode(&create_entry, &key_pair).unwrap();
    let encoded_operation = OperationEncoded::try_from(&create_operation).unwrap();
    let storage_entry = StorageEntry::new(&encoded_entry, &encoded_operation).unwrap();

    db_entries.push(storage_entry);

    // Create 9 more entries containing UPDATE operations with valid back- and skip- links and previous_operations
    for seq_num in 2..10 {
        let seq_num = SeqNum::new(seq_num).unwrap();
        let mut skiplink = None;
        let backlink = db_entries
            .get(seq_num.as_u64() as usize - 2)
            .unwrap()
            .entry_signed()
            .hash();

        if SKIPLINK_ENTRIES.contains(&seq_num.as_u64()) {
            let skiplink_seq_num = seq_num.skiplink_seq_num().unwrap();
            skiplink = Some(
                db_entries
                    .get(skiplink_seq_num.as_u64() as usize - 1)
                    .unwrap()
                    .entry_signed()
                    .hash(),
            );
        };

        let update_operation = update_operation(
            schema.clone(),
            vec![db_entries
                .get(seq_num.as_u64() as usize - 2)
                .unwrap()
                .hash()
                .into()],
            fields.clone(),
        );

        let update_entry = entry(update_operation.clone(), seq_num, Some(backlink), skiplink);

        let encoded_entry = sign_and_encode(&update_entry, &key_pair).unwrap();
        let encoded_operation = OperationEncoded::try_from(&update_operation).unwrap();
        let storage_entry = StorageEntry::new(&encoded_entry, &encoded_operation).unwrap();

        db_entries.push(storage_entry)
    }

    // Instantiate a SimpleStorage with the existing entry and log values stored.
    SimplestStorageProvider {
        logs: Arc::new(Mutex::new(db_logs)),
        entries: Arc::new(Mutex::new(db_entries.clone())),
    }
}

#[rstest]
#[async_std::test]
async fn test_the_test_db(test_db: SimplestStorageProvider) {
    let entries = test_db.entries.lock().unwrap().clone();
    for seq_num in 1..10 {
        let entry = entries.get(seq_num - 1).unwrap();

        let expected_seq_num = SeqNum::new(seq_num as u64).unwrap();
        assert_eq!(expected_seq_num, *entry.entry_decoded().seq_num());

        let expected_log_id = LogId::default();
        assert_eq!(expected_log_id, entry.log_id());

        let mut expected_backlink_hash = None;

        if seq_num != 1 {
            expected_backlink_hash = entries
                .get(seq_num - 2)
                .map(|backlink_entry| backlink_entry.hash());
        }
        assert_eq!(
            expected_backlink_hash,
            entry.entry_decoded().backlink_hash().cloned()
        );

        let mut expected_skiplink_hash = None;

        if SKIPLINK_ENTRIES.contains(&(seq_num as u64)) {
            let skiplink_seq_num = entry
                .entry_decoded()
                .seq_num()
                .skiplink_seq_num()
                .unwrap()
                .as_u64();

            let skiplink_entry = entries
                .get((skiplink_seq_num as usize) - 1)
                .unwrap()
                .clone();

            expected_skiplink_hash = Some(skiplink_entry.hash());
        };

        assert_eq!(expected_skiplink_hash, entry.skiplink_hash());
    }
}
