// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;

use rstest::fixture;

use crate::document::{DocumentId, DocumentViewId};
use crate::entry::{sign_and_encode, Entry, EntrySigned};
use crate::hash::Hash;
use crate::identity::{Author, KeyPair};
use crate::operation::{
    AsOperation, AsVerifiedOperation, Operation, OperationEncoded, OperationValue, PinnedRelation,
    PinnedRelationList, Relation, RelationList, VerifiedOperation,
};
use crate::schema::SchemaId;
use crate::storage_provider::traits::{OperationStore, StorageProvider};
use crate::storage_provider::utils::Result;
use crate::test_utils::constants::{PRIVATE_KEY, SCHEMA_ID};
use crate::test_utils::db::{
    EntryArgsRequest, MemoryStore, PublishEntryRequest, PublishEntryResponse, StorageLog,
};
use crate::test_utils::fixtures::{operation, operation_fields};

use super::{AsStorageLog, LogStore};

/// The fields used as defaults in the tests.
pub fn complex_test_fields() -> Vec<(&'static str, OperationValue)> {
    vec![
        ("username", OperationValue::Text("bubu".to_owned())),
        ("height", OperationValue::Float(3.5)),
        ("age", OperationValue::Integer(28)),
        ("is_admin", OperationValue::Boolean(false)),
        (
            "profile_picture",
            OperationValue::Relation(Relation::new(
                Hash::new("0020eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee")
                    .unwrap()
                    .into(),
            )),
        ),
        (
            "special_profile_picture",
            OperationValue::PinnedRelation(PinnedRelation::new(
                Hash::new("0020ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff")
                    .unwrap()
                    .into(),
            )),
        ),
        (
            "many_profile_pictures",
            OperationValue::RelationList(RelationList::new(vec![
                Hash::new("0020aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
                    .unwrap()
                    .into(),
                Hash::new("0020bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
                    .unwrap()
                    .into(),
            ])),
        ),
        (
            "many_special_profile_pictures",
            OperationValue::PinnedRelationList(PinnedRelationList::new(vec![
                Hash::new("0020cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc")
                    .unwrap()
                    .into(),
                Hash::new("0020dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd")
                    .unwrap()
                    .into(),
            ])),
        ),
        (
            "another_relation_field",
            OperationValue::PinnedRelationList(PinnedRelationList::new(vec![
                Hash::new("0020abababababababababababababababababababababababababababababababab")
                    .unwrap()
                    .into(),
                Hash::new("0020cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd")
                    .unwrap()
                    .into(),
            ])),
        ),
    ]
}

/// Configuration used in test database population.
#[derive(Debug)]
pub struct PopulateDatabaseConfig {
    /// Number of entries per log/document.
    pub no_of_entries: usize,

    /// Number of logs for each author.
    pub no_of_logs: usize,

    /// Number of authors, each with logs populated as defined above.
    pub no_of_authors: usize,

    /// A boolean flag for wether all logs should contain a delete operation.
    pub with_delete: bool,

    /// The schema used for all operations in the db.
    pub schema: SchemaId,

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
            no_of_authors: 0,
            with_delete: false,
            schema: SCHEMA_ID.parse().unwrap(),
            create_operation_fields: complex_test_fields(),
            update_operation_fields: complex_test_fields(),
        }
    }
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
    // Number of logs for each author
    #[default(0)] no_of_logs: usize,
    // Number of authors, each with logs populated as defined above
    #[default(0)] no_of_authors: usize,
    // A boolean flag for wether all logs should contain a delete operation
    #[default(false)] with_delete: bool,
    // The schema used for all operations in the db
    #[default(SCHEMA_ID.parse().unwrap())] schema: SchemaId,
    // The fields used for every CREATE operation
    #[default(complex_test_fields())] create_operation_fields: Vec<(&'static str, OperationValue)>,
    // The fields used for every UPDATE operation
    #[default(complex_test_fields())] update_operation_fields: Vec<(&'static str, OperationValue)>,
) -> TestStore {
    let config = PopulateDatabaseConfig {
        no_of_entries,
        no_of_logs,
        no_of_authors,
        with_delete,
        schema,
        create_operation_fields,
        update_operation_fields,
    };

    let mut db = TestStore::default();
    populate_test_db(&mut db, &config).await;
    db
}

/// Helper for creating many key_pairs.
///
/// If there is only one key_pair in the list it will always be the default testing
/// key pair.
pub fn test_key_pairs(no_of_authors: usize) -> Vec<KeyPair> {
    let mut key_pairs = Vec::new();
    match no_of_authors {
        0 => (),
        1 => key_pairs.push(KeyPair::from_private_key_str(PRIVATE_KEY).unwrap()),
        _ => {
            key_pairs.push(KeyPair::from_private_key_str(PRIVATE_KEY).unwrap());
            for _index in 2..no_of_authors {
                key_pairs.push(KeyPair::new())
            }
        }
    };
    key_pairs
}

/// Container for `MemoryStore` with access to the document ids and key_pairs present in the
/// pre-populated database.
#[derive(Default, Debug)]
pub struct TestStore {
    /// The store.
    pub store: MemoryStore,

    /// Test data collected during store population.
    pub test_data: TestData,
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

/// Helper method for populating a `TestStore` with configurable data.
///
/// Passed parameters define what the db should contain. The first entry in each log contains a
/// valid CREATE operation following entries contain duplicate UPDATE operations. If the
/// with_delete flag is set to true the last entry in all logs contain be a DELETE operation.
pub async fn populate_test_db(db: &mut TestStore, config: &PopulateDatabaseConfig) {
    let key_pairs = test_key_pairs(config.no_of_authors);

    for key_pair in &key_pairs {
        db.test_data
            .key_pairs
            .push(KeyPair::from_private_key(key_pair.private_key()).unwrap());

        for _log_id in 0..config.no_of_logs {
            let mut previous_operation: Option<DocumentViewId> = None;

            for index in 0..config.no_of_entries {
                // Create an operation based on the current index and whether this document should
                // contain a DELETE operation
                let next_operation_fields = match index {
                    // First operation is CREATE
                    0 => Some(operation_fields(config.create_operation_fields.clone())),
                    // Last operation is DELETE if the with_delete flag is set
                    seq if seq == (config.no_of_entries - 1) && config.with_delete => None,
                    // All other operations are UPDATE
                    _ => Some(operation_fields(config.update_operation_fields.clone())),
                };

                // Publish the operation encoded on an entry to storage.
                let (entry_encoded, publish_entry_response) = send_to_store(
                    &db.store,
                    &operation(
                        next_operation_fields,
                        previous_operation,
                        Some(config.schema.to_owned()),
                    ),
                    key_pair,
                )
                .await
                .unwrap();

                // Set the previous_operations based on the backlink
                previous_operation = publish_entry_response.backlink.map(DocumentViewId::from);

                // If this was the first entry in the document, store the doucment id for later.
                if index == 0 {
                    let document_id = Some(entry_encoded.hash().into());
                    db.test_data.documents.push(document_id.clone().unwrap());
                }
            }
        }
    }
}

/// Helper method for publishing an operation encoded on an entry to a store.
pub async fn send_to_store(
    store: &MemoryStore,
    operation: &Operation,
    key_pair: &KeyPair,
) -> Result<(EntrySigned, PublishEntryResponse)> {
    // Get an Author from the key_pair.
    let author = Author::try_from(key_pair.public_key().to_owned())?;

    let document_id = if operation.is_create() {
        None
    } else {
        store
            .get_document_by_entry(
                operation
                    .previous_operations()
                    .unwrap()
                    .into_iter()
                    .next()
                    .unwrap()
                    .as_hash(),
            )
            .await?
    };

    // Get the next entry arguments for this author and the passed document id.
    let next_entry_args = store
        .get_entry_args(&EntryArgsRequest {
            public_key: author.clone(),
            document_id: document_id.clone(),
        })
        .await?;

    // Construct the next entry.
    let next_entry = Entry::new(
        &next_entry_args.log_id,
        Some(operation),
        next_entry_args.skiplink.map(Hash::from).as_ref(),
        next_entry_args.backlink.map(Hash::from).as_ref(),
        &next_entry_args.seq_num,
    )?;

    // Encode both the entry and operation.
    let entry = sign_and_encode(&next_entry, key_pair)?;
    let operation_encoded = OperationEncoded::try_from(operation)?;

    // Publish the entry and get the next entry args.
    let publish_entry_request = PublishEntryRequest {
        entry: entry.clone(),
        operation: operation_encoded,
    };
    let publish_entry_response = store.publish_entry(&publish_entry_request).await?;

    let document_id = {
        let default = entry.hash().into();
        match document_id {
            Some(id) => id,
            None => default,
        }
    };

    // Insert the log into the store.
    store
        .insert_log(StorageLog::new(
            &entry.author(),
            &operation.schema(),
            &document_id,
            &next_entry_args.log_id,
        ))
        .await?;

    // Also insert the operation into the store.
    let verified_operation = VerifiedOperation::new(&author, &entry.hash().into(), operation)?;
    store
        .insert_operation(&verified_operation, &document_id)
        .await?;

    Ok((entry, publish_entry_response))
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::entry::{LogId, SeqNum};
    use crate::storage_provider::traits::{test_utils::test_db, AsStorageEntry};
    use crate::test_utils::constants::SKIPLINK_SEQ_NUMS;

    use super::TestStore;

    #[rstest]
    #[tokio::test]
    async fn test_the_test_db(
        #[from(test_db)]
        #[with(17, 1, 1)]
        #[future]
        db: TestStore,
    ) {
        let db = db.await;
        let entries = db.store.entries.lock().unwrap().clone();
        for seq_num in 1..17 {
            let entry = entries
                .values()
                .find(|entry| entry.seq_num().as_u64() as usize == seq_num)
                .unwrap();

            let expected_seq_num = SeqNum::new(seq_num as u64).unwrap();
            assert_eq!(expected_seq_num, *entry.entry_decoded().seq_num());

            let expected_log_id = LogId::default();
            assert_eq!(expected_log_id, entry.log_id());

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
            assert_eq!(expected_backlink_hash, entry.backlink_hash());

            let mut expected_skiplink_hash = None;

            if SKIPLINK_SEQ_NUMS.contains(&(seq_num as u64)) {
                let skiplink_seq_num = entry.seq_num().skiplink_seq_num().unwrap().as_u64();

                let skiplink_entry = entries
                    .values()
                    .find(|entry| entry.seq_num().as_u64() == skiplink_seq_num)
                    .unwrap();
                expected_skiplink_hash = Some(skiplink_entry.hash());
            };

            assert_eq!(expected_skiplink_hash, entry.skiplink_hash());
        }
    }
}
