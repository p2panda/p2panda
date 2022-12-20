// SPDX-License-Identifier: AGPL-3.0-or-later

//! Helper methods for preparing test databases.
use rstest::fixture;

use crate::document::{DocumentId, DocumentViewId};
use crate::entry::encode::{encode_entry, sign_entry};
use crate::entry::traits::AsEncodedEntry;
use crate::entry::EncodedEntry;
use crate::identity::KeyPair;
use crate::operation::encode::encode_operation;
use crate::operation::traits::Actionable;
use crate::operation::{Operation, OperationAction, OperationBuilder, OperationValue};
use crate::schema::Schema;
use crate::storage_provider::traits::{EntryStore, LogStore, OperationStore};
use crate::storage_provider::utils::Result;
use crate::test_utils::constants;
use crate::test_utils::db::{EntryArgsResponse, MemoryStore};
use crate::test_utils::fixtures::schema;

use super::domain::{next_args, publish};

/// Configuration used in test database population.
#[derive(Debug)]
pub struct PopulateDatabaseConfig {
    /// Number of entries per log/document.
    pub no_of_entries: usize,

    /// Number of logs for each public key.
    pub no_of_logs: usize,

    /// Number of public keys, each with logs populated as defined above.
    pub no_of_public_keys: usize,

    /// A boolean flag for wether all logs should contain a delete operation.
    pub with_delete: bool,

    /// The schema used for all operations in the db.
    pub schema: Schema,

    /// The fields used for every CREATE operation.
    pub create_operation_fields: Vec<(&'static str, OperationValue)>,

    /// The fields used for every UPDATE operation.
    pub update_operation_fields: Vec<(&'static str, OperationValue)>,
}

impl Default for PopulateDatabaseConfig {
    fn default() -> Self {
        Self {
            no_of_entries: 0,
            no_of_logs: 0,
            no_of_public_keys: 0,
            with_delete: false,
            schema: constants::schema(),
            create_operation_fields: constants::test_fields(),
            update_operation_fields: constants::test_fields(),
        }
    }
}

/// Fixture for passing in `PopulateDatabaseConfig` into tests.
#[fixture]
pub fn test_db_config(
    // Number of entries per log/document
    #[default(0)] no_of_entries: usize,
    // Number of logs for each public key
    #[default(0)] no_of_logs: usize,
    // Number of public keys, each with logs populated as defined above
    #[default(0)] no_of_public_keys: usize,
    // A boolean flag for whether all logs should contain a delete operation
    #[default(false)] with_delete: bool,
    // The schema used for all operations in the db
    #[from(schema)] schema: Schema,
    // The fields used for every CREATE operation
    #[default(constants::test_fields())] create_operation_fields: Vec<(
        &'static str,
        OperationValue,
    )>,
    // The fields used for every UPDATE operation
    #[default(constants::test_fields())] update_operation_fields: Vec<(
        &'static str,
        OperationValue,
    )>,
) -> PopulateDatabaseConfig {
    PopulateDatabaseConfig {
        no_of_entries,
        no_of_logs,
        no_of_public_keys,
        with_delete,
        schema,
        create_operation_fields,
        update_operation_fields,
    }
}

/// Container for `MemoryStore` with access to the document ids and key_pairs present in the
/// pre-populated database.
#[derive(Default, Debug)]
pub struct TestDatabase {
    /// The store.
    pub store: MemoryStore,

    /// Test data collected during store population.
    pub test_data: TestData,
}

impl TestDatabase {
    /// Instantiate a new test store.
    pub fn new(store: &MemoryStore, test_data: TestData) -> Self {
        Self {
            store: store.clone(),
            test_data,
        }
    }
}

/// Data collected when populating a `TestData` base in order to easily check values which
/// would be otherwise hard or impossible to get through the store methods.
///
/// Note: if new entries are published to this node, keypairs and any newly created
/// documents will not be added to these lists.
#[derive(Default, Debug)]
pub struct TestData {
    /// KeyPairs which were used to pre-populate this store.
    pub key_pairs: Vec<KeyPair>,

    /// The id of all documents which were inserted into the store when it was
    /// pre-populated with values.
    pub documents: Vec<DocumentId>,
}

/// Fixture for constructing a storage provider instance backed by a pre-populated database.
///
/// Passed parameters define what the database should contain. The first entry in each log contains
/// a valid CREATE operation following entries contain UPDATE operations. If the with_delete
///  flag is set to true the last entry in all logs contain be a DELETE operation.
#[fixture]
pub async fn test_db(
    // Number of entries per log/document
    #[default(0)] no_of_entries: usize,
    // Number of logs for each public key
    #[default(0)] no_of_logs: usize,
    // Number of public keys, each with logs populated as defined above
    #[default(0)] no_of_public_keys: usize,
    // A boolean flag for wether all logs should contain a delete operation
    #[default(false)] with_delete: bool,
    // The schema used for all operations in the db
    #[from(schema)] schema: Schema,
    // The fields used for every CREATE operation
    #[default(constants::test_fields())] create_operation_fields: Vec<(
        &'static str,
        OperationValue,
    )>,
    // The fields used for every UPDATE operation
    #[default(constants::test_fields())] update_operation_fields: Vec<(
        &'static str,
        OperationValue,
    )>,
) -> TestDatabase {
    let config = PopulateDatabaseConfig {
        no_of_entries,
        no_of_logs,
        no_of_public_keys,
        with_delete,
        schema,
        create_operation_fields,
        update_operation_fields,
    };

    let store = MemoryStore::default();
    let (key_pairs, documents) = populate_store(&store, &config).await;
    TestDatabase::new(
        &store,
        TestData {
            key_pairs,
            documents,
        },
    )
}

/// Helper for creating many key_pairs.
///
/// If there is only one key_pair in the list it will always be the default testing
/// key pair.
pub fn test_key_pairs(no_of_public_keys: usize) -> Vec<KeyPair> {
    let mut key_pairs = Vec::new();
    match no_of_public_keys {
        0 => (),
        1 => key_pairs.push(KeyPair::from_private_key_str(constants::PRIVATE_KEY).unwrap()),
        _ => {
            key_pairs.push(KeyPair::from_private_key_str(constants::PRIVATE_KEY).unwrap());
            for _index in 1..no_of_public_keys {
                key_pairs.push(KeyPair::new())
            }
        }
    };
    key_pairs
}

/// Helper method for populating a `TestDatabase` with configurable data.
///
/// Passed parameters define what the db should contain. The first entry in each log contains a
/// valid CREATE operation following entries contain duplicate UPDATE operations. If the
/// with_delete flag is set to true the last entry in all logs contain be a DELETE operation.
pub async fn populate_store<S: EntryStore + LogStore + OperationStore>(
    store: &S,
    config: &PopulateDatabaseConfig,
) -> (Vec<KeyPair>, Vec<DocumentId>) {
    let key_pairs = test_key_pairs(config.no_of_public_keys);
    let mut documents: Vec<DocumentId> = Vec::new();
    for key_pair in &key_pairs {
        for _log_id in 0..config.no_of_logs {
            let mut previous: Option<DocumentViewId> = None;

            for index in 0..config.no_of_entries {
                // Create an operation based on the current index and whether this document should
                // contain a DELETE operation
                let operation = match index {
                    // First operation is CREATE
                    0 => OperationBuilder::new(config.schema.id())
                        .fields(&config.create_operation_fields)
                        .build()
                        .expect("Error building operation"),
                    // Last operation is DELETE if the with_delete flag is set
                    seq if seq == (config.no_of_entries - 1) && config.with_delete => {
                        OperationBuilder::new(config.schema.id())
                            .action(OperationAction::Delete)
                            .previous(&previous.expect("Previous should be set"))
                            .build()
                            .expect("Error building operation")
                    }
                    // All other operations are UPDATE
                    _ => OperationBuilder::new(config.schema.id())
                        .action(OperationAction::Update)
                        .fields(&config.update_operation_fields)
                        .previous(&previous.expect("Previous should be set"))
                        .build()
                        .expect("Error building operation"),
                };

                // Publish the operation encoded on an entry to storage.
                let (entry_encoded, publish_entry_response) =
                    send_to_store(store, &operation, &config.schema, key_pair)
                        .await
                        .expect("Send to store");

                // Set the previous based on the backlink
                previous = publish_entry_response.backlink.map(DocumentViewId::from);

                // Push this document id to the test data.
                if index == 0 {
                    documents.push(entry_encoded.hash().into());
                }
            }
        }
    }
    (key_pairs, documents)
}

/// Helper method for publishing an operation encoded on an entry to a store.
pub async fn send_to_store<S: EntryStore + LogStore + OperationStore>(
    store: &S,
    operation: &Operation,
    schema: &Schema,
    key_pair: &KeyPair,
) -> Result<(EncodedEntry, EntryArgsResponse)> {
    // Get public key from the key pair.
    let public_key = key_pair.public_key();

    // Get the next args.
    let next_args = next_args(store, &public_key, operation.previous()).await?;

    // Encode the operation.
    let encoded_operation = encode_operation(operation)?;

    // Construct and sign the entry.
    let entry = sign_entry(
        &next_args.log_id,
        &next_args.seq_num,
        next_args.skiplink.as_ref(),
        next_args.backlink.as_ref(),
        &encoded_operation,
        key_pair,
    )?;

    // Encode the entry.
    let encoded_entry = encode_entry(&entry)?;

    // Publish the entry and get the next entry args.
    let publish_entry_response = publish(
        store,
        schema,
        &encoded_entry,
        &operation.into(),
        &encoded_operation,
    )
    .await?;

    Ok((encoded_entry, publish_entry_response))
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::entry::traits::{AsEncodedEntry, AsEntry};
    use crate::entry::{LogId, SeqNum};
    use crate::test_utils::constants::SKIPLINK_SEQ_NUMS;

    use super::{test_db, TestDatabase};

    #[rstest]
    #[tokio::test]
    async fn correct_next_args(
        #[from(test_db)]
        #[with(17, 1, 1)]
        #[future]
        db: TestDatabase,
    ) {
        let db = db.await;
        let entries = db.store.entries.lock().unwrap().clone();
        for seq_num in 1..17 {
            let entry = entries
                .values()
                .find(|entry| entry.seq_num().as_u64() as usize == seq_num)
                .unwrap();

            let expected_seq_num = SeqNum::new(seq_num as u64).unwrap();
            assert_eq!(expected_seq_num, *entry.seq_num());

            let expected_log_id = LogId::default();
            assert_eq!(expected_log_id, *entry.log_id());

            let mut expected_backlink_hash = None;

            if seq_num != 1 {
                expected_backlink_hash = Some(
                    entries
                        .values()
                        .find(|entry| entry.seq_num().as_u64() as usize == seq_num - 1)
                        .unwrap()
                        .hash(),
                );
            }
            assert_eq!(expected_backlink_hash.as_ref(), entry.backlink());

            let mut expected_skiplink_hash = None;

            if SKIPLINK_SEQ_NUMS.contains(&(seq_num as u64)) {
                let skiplink_seq_num = entry.seq_num().skiplink_seq_num().unwrap().as_u64();

                let skiplink_entry = entries
                    .values()
                    .find(|entry| entry.seq_num().as_u64() == skiplink_seq_num)
                    .unwrap();
                expected_skiplink_hash = Some(skiplink_entry.hash());
            };

            assert_eq!(expected_skiplink_hash.as_ref(), entry.skiplink());
        }
    }

    #[rstest]
    #[tokio::test]
    async fn correct_test_values(
        #[from(test_db)]
        #[with(10, 4, 2)]
        #[future]
        db: TestDatabase,
    ) {
        let db = db.await;
        assert_eq!(db.test_data.key_pairs.len(), 2);
        assert_eq!(db.test_data.documents.len(), 8);
        assert_eq!(db.store.entries.lock().unwrap().len(), 80);
        assert_eq!(db.store.operations.lock().unwrap().len(), 80);
        assert_eq!(db.store.documents.lock().unwrap().len(), 0);
        assert_eq!(db.store.document_views.lock().unwrap().len(), 0);
    }
}
