// SPDX-License-Identifier: AGPL-3.0-or-later

//! Helper methods for working with a storage provider when testing.

use crate::api::{next_args, publish};
use crate::document::{DocumentId, DocumentViewId};
use crate::entry::encode::{encode_entry, sign_entry};
use crate::entry::traits::{AsEncodedEntry, AsEntry};
use crate::entry::{EncodedEntry, LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::{KeyPair, PublicKey};
use crate::operation::encode::encode_operation;
use crate::operation::traits::Actionable;
use crate::operation::{Operation, OperationAction, OperationBuilder, OperationValue};
use crate::schema::Schema;
use crate::storage_provider::traits::{EntryStore, LogStore, OperationStore};
use crate::storage_provider::utils::Result;
use crate::test_utils::constants;

use super::MemoryStore;

/// A skiplink hash.
type Skiplink = Hash;

/// A backlink hash.
type Backlink = Hash;

/// The next args returned from `publish` and `next_args`.
type NextArgs = (Option<Backlink>, Option<Skiplink>, SeqNum, LogId);

/// Configuration used when populating the store for testing.
#[derive(Debug)]
pub struct PopulateStoreConfig {
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

impl Default for PopulateStoreConfig {
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

/// Helper for creating many key_pairs.
///
/// If there is only one key_pair in the list it will always be the default testing
/// key pair.
pub fn many_key_pairs(no_of_public_keys: usize) -> Vec<KeyPair> {
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

/// Helper method for populating the store with test data.
///
/// Passed parameters define what the store should contain. The first entry in each log contains a
/// valid CREATE operation following entries contain UPDATE operations. If the with_delete flag is set
/// to true the last entry in all logs contain be a DELETE operation.
pub async fn populate_store<S: EntryStore + LogStore + OperationStore>(
    store: &S,
    config: &PopulateStoreConfig,
) -> (Vec<KeyPair>, Vec<DocumentId>) {
    let key_pairs = many_key_pairs(config.no_of_public_keys);
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
                let (entry_encoded, (backlink, _, _, _)) =
                    send_to_store(store, &operation, &config.schema, key_pair)
                        .await
                        .expect("Send to store");

                // Set the previous based on the backlink
                previous = backlink.map(DocumentViewId::from);

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
) -> Result<(EncodedEntry, NextArgs)> {
    // Get public key from the key pair.
    let public_key = key_pair.public_key();

    // Get the next args.
    let (backlink, skiplink, seq_num, log_id) =
        next_args(store, &public_key, operation.previous()).await?;

    // Encode the operation.
    let encoded_operation = encode_operation(operation)?;

    // Construct and sign the entry.
    let entry = sign_entry(
        &log_id,
        &seq_num,
        skiplink.as_ref(),
        backlink.as_ref(),
        &encoded_operation,
        key_pair,
    )?;

    // Encode the entry.
    let encoded_entry = encode_entry(&entry)?;

    // Publish the entry and get the next entry args.
    let (backlink, skiplink, seq_num, log_id) = publish(
        store,
        schema,
        &encoded_entry,
        &operation.into(),
        &encoded_operation,
    )
    .await?;

    Ok((encoded_entry, (backlink, skiplink, seq_num, log_id)))
}


type LogIdAndSeqNum = (u64, u64);

/// Helper method for removing entries from a MemoryStore by PublicKey & LogIdAndSeqNum.
pub fn remove_entries(
    store: &MemoryStore,
    public_key: &PublicKey,
    entries_to_remove: &[LogIdAndSeqNum],
) {
    store.entries.lock().unwrap().retain(|_, entry| {
        !entries_to_remove.contains(&(entry.log_id().as_u64(), entry.seq_num().as_u64()))
            && entry.public_key() == public_key
    });
}

/// Helper method for removing operations from a MemoryStore by PublicKey & LogIdAndSeqNum.
pub fn remove_operations(
    store: &MemoryStore,
    public_key: &PublicKey,
    operations_to_remove: &[LogIdAndSeqNum],
) {
    for (hash, entry) in store.entries.lock().unwrap().iter() {
        if operations_to_remove.contains(&(entry.log_id().as_u64(), entry.seq_num().as_u64()))
            && entry.public_key() == public_key
        {
            store
                .operations
                .lock()
                .unwrap()
                .remove(&hash.clone().into());
        }
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::document::DocumentViewId;
    use crate::entry::traits::{AsEncodedEntry, AsEntry};
    use crate::entry::{LogId, SeqNum};
    use crate::identity::KeyPair;
    use crate::operation::Operation;
    use crate::schema::Schema;
    use crate::storage_provider::traits::DocumentStore;
    use crate::test_utils::constants::{test_fields, SKIPLINK_SEQ_NUMS};
    use crate::test_utils::fixtures::{
        key_pair, operation, populate_store_config, random_key_pair, schema, update_operation,
    };
    use crate::test_utils::memory_store::helpers::{
        populate_store, send_to_store, PopulateStoreConfig,
    };
    use crate::test_utils::memory_store::MemoryStore;

    #[rstest]
    #[tokio::test]
    async fn correct_next_args(
        #[from(populate_store_config)]
        #[with(17, 1, 1)]
        config: PopulateStoreConfig,
    ) {
        let store = MemoryStore::default();
        populate_store(&store, &config).await;

        let entries = store.entries.lock().unwrap().clone();
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
        schema: Schema,
        #[from(populate_store_config)]
        #[with(10, 4, 2)]
        config: PopulateStoreConfig,
    ) {
        let store = MemoryStore::default();
        let (key_pairs, documents) = populate_store(&store, &config).await;

        assert_eq!(key_pairs.len(), 2);
        assert_eq!(documents.len(), 8);
        assert_eq!(store.entries.lock().unwrap().len(), 80);
        assert_eq!(store.operations.lock().unwrap().len(), 80);
        assert_eq!(
            store
                .get_documents_by_schema(schema.id())
                .await
                .unwrap()
                .len(),
            8
        );
    }

    #[rstest]
    #[tokio::test]
    async fn sends_to_node(
        operation: Operation,
        schema: Schema,
        key_pair: KeyPair,
        #[from(random_key_pair)] another_key_pair: KeyPair,
    ) {
        let store = MemoryStore::default();

        // Publish first entry and operation.
        let (encoded_entry, (backlink, skiplink, seq_num, log_id)) =
            send_to_store(&store, &operation, &schema, &key_pair)
                .await
                .unwrap();

        assert_eq!(seq_num, SeqNum::new(2).unwrap());
        assert_eq!(log_id, LogId::new(0));
        assert!(backlink.is_some());
        assert_eq!(backlink.clone().unwrap(), encoded_entry.hash());
        assert!(skiplink.is_none());

        let update = update_operation(
            test_fields(),
            backlink.map(DocumentViewId::from).unwrap(),
            schema.id().clone(),
        );

        // Publish second entry and an update operation.
        let (encoded_entry, (backlink, skiplink, seq_num, log_id)) =
            send_to_store(&store, &update, &schema, &key_pair)
                .await
                .unwrap();

        assert_eq!(seq_num, SeqNum::new(3).unwrap());
        assert_eq!(log_id, LogId::new(0));
        assert!(backlink.is_some());
        assert_eq!(backlink.unwrap(), encoded_entry.hash());
        assert!(skiplink.is_none());

        // Publish an entry and operation to a new log.
        let (encoded_entry, (backlink, skiplink, seq_num, log_id)) =
            send_to_store(&store, &operation, &schema, &key_pair)
                .await
                .unwrap();

        assert_eq!(seq_num, SeqNum::new(2).unwrap());
        assert_eq!(log_id, LogId::new(1));
        assert!(backlink.is_some());
        assert_eq!(backlink.clone().unwrap(), encoded_entry.hash());
        assert!(skiplink.is_none());

        // Publish an entry and operation with a new key pair.
        let (encoded_entry, (backlink, skiplink, seq_num, log_id)) =
            send_to_store(&store, &operation, &schema, &another_key_pair)
                .await
                .unwrap();

        assert_eq!(seq_num, SeqNum::new(2).unwrap());
        assert_eq!(log_id, LogId::new(0));
        assert!(backlink.is_some());
        assert_eq!(backlink.clone().unwrap(), encoded_entry.hash());
        assert!(skiplink.is_none());
    }
}
