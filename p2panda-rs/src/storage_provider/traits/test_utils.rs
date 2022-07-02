// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;

use rstest::{fixture, rstest};

use crate::document::DocumentId;
use crate::entry::{sign_and_encode, Entry, LogId, SeqNum};
use crate::identity::{Author, KeyPair};
use crate::operation::{AsVerifiedOperation, OperationEncoded, OperationValue, VerifiedOperation};
use crate::schema::SchemaId;
use crate::test_utils::constants::{default_fields, DEFAULT_PRIVATE_KEY, TEST_SCHEMA_ID};
use crate::test_utils::db::{EntryArgsRequest, SimplestStorageProvider, StorageEntry, StorageLog};
use crate::test_utils::fixtures::{create_operation, delete_operation, update_operation};

use super::{AsStorageEntry, AsStorageLog, EntryStore, LogStore, OperationStore, StorageProvider};

pub const SKIPLINK_ENTRIES: [u64; 5] = [4, 8, 12, 13, 17];

/// Helper for creating many key_pairs.
pub fn test_key_pairs(no_of_authors: usize) -> Vec<KeyPair> {
    let mut key_pairs = vec![KeyPair::from_private_key_str(DEFAULT_PRIVATE_KEY).unwrap()];

    for _index in 1..no_of_authors {
        key_pairs.push(KeyPair::new())
    }

    key_pairs
}

/// Container for the test store with access to the document ids and key_pairs
/// present in the pre-populated database.
pub struct TestStore {
    pub store: SimplestStorageProvider,
    pub key_pairs: Vec<KeyPair>,
    pub documents: Vec<DocumentId>,
}

/// Fixture for constructing a storage provider instance backed by a pre-polpulated database. Passed
/// parameters define what the db should contain. The first entry in each log contains a valid CREATE
/// operation following entries contain duplicate UPDATE operations. If the with_delete flag is set
/// to true the last entry in all logs contain be a DELETE operation.
///
/// Returns a `TestStore` containing storage provider instance, a vector of key pairs for all authors
/// in the db, and a vector of the ids for all documents.
#[fixture]
pub async fn test_db(
    // Number of entries per log/document
    #[default(0)] no_of_entries: usize,
    // Number of authors, each with a log populated as defined above
    #[default(0)] no_of_authors: usize,
    // A boolean flag for wether all logs should contain a delete operation
    #[default(false)] with_delete: bool,
    // The schema used for all operations in the db
    #[default(TEST_SCHEMA_ID.parse().unwrap())] schema: SchemaId,
    // The fields used for every CREATE operation
    #[default(default_fields())] create_operation_fields: Vec<(&'static str, OperationValue)>,
    // The fields used for every UPDATE operation
    #[default(default_fields())] update_operation_fields: Vec<(&'static str, OperationValue)>,
) -> TestStore {
    let mut documents: Vec<DocumentId> = Vec::new();
    let key_pairs = test_key_pairs(no_of_authors);

    let store = SimplestStorageProvider::default();

    // If we don't want any entries in the db return now
    if no_of_entries == 0 {
        return TestStore {
            store,
            key_pairs,
            documents,
        };
    }

    for key_pair in &key_pairs {
        let mut document: Option<DocumentId> = None;
        let author = Author::try_from(key_pair.public_key().to_owned()).unwrap();
        for index in 0..no_of_entries {
            let next_entry_args = store
                .get_entry_args(&EntryArgsRequest {
                    author: author.clone(),
                    document: document.as_ref().cloned(),
                })
                .await
                .unwrap();

            let next_operation = if index == 0 {
                create_operation(&create_operation_fields)
            } else if index == (no_of_entries - 1) && with_delete {
                delete_operation(&next_entry_args.entry_hash_backlink.clone().unwrap().into())
            } else {
                update_operation(
                    &update_operation_fields,
                    &next_entry_args.entry_hash_backlink.clone().unwrap().into(),
                )
            };

            let next_entry = Entry::new(
                &next_entry_args.log_id,
                Some(&next_operation),
                next_entry_args.entry_hash_skiplink.as_ref(),
                next_entry_args.entry_hash_backlink.as_ref(),
                &next_entry_args.seq_num,
            )
            .unwrap();

            let entry_encoded = sign_and_encode(&next_entry, key_pair).unwrap();
            let operation_encoded = OperationEncoded::try_from(&next_operation).unwrap();

            if index == 0 {
                document = Some(entry_encoded.hash().into());
                documents.push(entry_encoded.hash().into());
            }

            let storage_entry = StorageEntry::new(&entry_encoded, &operation_encoded).unwrap();

            store.insert_entry(storage_entry).await.unwrap();

            let storage_log = StorageLog::new(
                &author,
                &schema,
                &document.clone().unwrap(),
                &next_entry_args.log_id,
            );

            if next_entry_args.seq_num.is_first() {
                store.insert_log(storage_log).await.unwrap();
            }

            let verified_operation =
                VerifiedOperation::new_from_entry(&entry_encoded, &operation_encoded).unwrap();

            store
                .insert_operation(&verified_operation, &document.clone().unwrap())
                .await
                .unwrap();
        }
    }
    TestStore {
        store,
        key_pairs,
        documents,
    }
}

#[rstest]
#[async_std::test]
async fn test_the_test_db(
    #[from(test_db)]
    #[with(17, 1)]
    #[future]
    db: TestStore,
) {
    let db = db.await;
    let entries = db.store.entries.lock().unwrap().clone();
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
