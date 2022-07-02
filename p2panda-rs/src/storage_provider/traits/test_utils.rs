// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;

use crate::document::{DocumentBuilder, DocumentId, DocumentViewId};
use crate::entry::{sign_and_encode, Entry, EntrySigned, LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::{Author, KeyPair};
use crate::operation::{
    AsOperation, AsVerifiedOperation, Operation, OperationEncoded, OperationId, OperationValue,
    PinnedRelation, PinnedRelationList, Relation, RelationList, VerifiedOperation,
};
use crate::schema::SchemaId;
use crate::storage_provider::traits::{AsStorageEntry, DocumentStore};
use crate::storage_provider::traits::{OperationStore, StorageProvider};
use crate::test_utils::constants::{DEFAULT_PRIVATE_KEY, TEST_SCHEMA_ID};
use crate::test_utils::db::{
    EntryArgsRequest, PublishEntryRequest, PublishEntryResponse, SimplestStorageProvider,
};
use crate::test_utils::fixtures::{operation, operation_fields};

use rstest::{fixture, rstest};

pub const SKIPLINK_ENTRIES: [u64; 5] = [4, 8, 12, 13, 17];

/// The fields used as defaults in the tests.
pub fn doggo_test_fields() -> Vec<(&'static str, OperationValue)> {
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

/// Helper for creating many key_pairs.
pub fn test_key_pairs(no_of_authors: usize) -> Vec<KeyPair> {
    let mut key_pairs = Vec::new();
    match no_of_authors {
        0 => (),
        1 => key_pairs.push(KeyPair::from_private_key_str(DEFAULT_PRIVATE_KEY).unwrap()),
        _ => {
            key_pairs.push(KeyPair::from_private_key_str(DEFAULT_PRIVATE_KEY).unwrap());
            for _index in 2..no_of_authors {
                key_pairs.push(KeyPair::new())
            }
        }
    };
    key_pairs
}

/// Helper for constructing a publish entry request.
pub async fn construct_publish_entry_request(
    provider: &SimplestStorageProvider,
    operation: &Operation,
    key_pair: &KeyPair,
    document_id: Option<&DocumentId>,
) -> PublishEntryRequest {
    let author = Author::try_from(key_pair.public_key().to_owned()).unwrap();
    let entry_args_request = EntryArgsRequest {
        public_key: author.clone(),
        document_id: document_id.cloned(),
    };
    let next_entry_args = provider.get_entry_args(&entry_args_request).await.unwrap();

    let entry = Entry::new(
        &next_entry_args.log_id,
        Some(operation),
        next_entry_args.skiplink.map(Hash::from).as_ref(),
        next_entry_args.backlink.map(Hash::from).as_ref(),
        &next_entry_args.seq_num,
    )
    .unwrap();

    let entry = sign_and_encode(&entry, key_pair).unwrap();
    let operation = OperationEncoded::try_from(operation).unwrap();
    PublishEntryRequest { entry, operation }
}

/// Helper for inserting an entry, operation and document_view into the database.
pub async fn insert_entry_operation_and_view(
    provider: &SimplestStorageProvider,
    key_pair: &KeyPair,
    document_id: Option<&DocumentId>,
    operation: &Operation,
) -> (DocumentId, DocumentViewId) {
    if !operation.is_create() && document_id.is_none() {
        panic!("UPDATE and DELETE operations require a DocumentId to be passed")
    }

    let request = construct_publish_entry_request(provider, operation, key_pair, document_id).await;

    let operation_id: OperationId = request.entry.hash().into();
    let document_id = document_id
        .cloned()
        .unwrap_or_else(|| request.entry.hash().into());

    let document_view_id: DocumentViewId = request.entry.hash().into();

    let author = Author::try_from(key_pair.public_key().to_owned()).unwrap();

    provider.publish_entry(&request).await.unwrap();
    provider
        .insert_operation(
            &VerifiedOperation::new(&author, &operation_id, operation).unwrap(),
            &document_id,
        )
        .await
        .unwrap();

    let document_operations = provider
        .get_operations_by_document_id(&document_id)
        .await
        .unwrap();

    let document = DocumentBuilder::new(document_operations).build().unwrap();

    provider.insert_document(&document).await.unwrap();

    (document_id, document_view_id)
}

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
            schema: TEST_SCHEMA_ID.parse().unwrap(),
            create_operation_fields: doggo_test_fields(),
            update_operation_fields: doggo_test_fields(),
        }
    }
}

/// Fixture for constructing a storage provider instance backed by a pre-populated database.
///
/// Returns a `TestStoreRunner` which allows to bootstrap a safe async test environment
/// connecting to a database. It makes sure the runner disconnects properly from the connection
/// pool after the test succeeded or even failed.
///
/// Passed parameters define what the database should contain. The first entry in each log contains
/// a valid CREATE operation following entries contain duplicate UPDATE operations. If the
/// with_delete flag is set to true the last entry in all logs contain be a DELETE operation.
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
    #[default(TEST_SCHEMA_ID.parse().unwrap())] schema: SchemaId,
    // The fields used for every CREATE operation
    #[default(doggo_test_fields())] create_operation_fields: Vec<(&'static str, OperationValue)>,
    // The fields used for every UPDATE operation
    #[default(doggo_test_fields())] update_operation_fields: Vec<(&'static str, OperationValue)>,
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

/// Container for `SqlStore` with access to the document ids and key_pairs used in the
/// pre-populated database for testing.
#[derive(Default, Debug, Clone)]
pub struct TestStore {
    pub store: SimplestStorageProvider,
    pub test_data: TestData,
}

/// Data collected when populating a `TestData` base in order to easily check values which
/// would be otherwise hard or impossible to get through the store methods.
#[derive(Default, Debug)]
pub struct TestData {
    pub key_pairs: Vec<KeyPair>,
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
            let mut document_id: Option<DocumentId> = None;
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
                    document_id.as_ref(),
                    key_pair,
                )
                .await;

                // Set the previous_operations based on the backlink
                previous_operation = publish_entry_response.backlink.map(DocumentViewId::from);

                // If this was the first entry in the document, store the doucment id for later.
                if index == 0 {
                    document_id = Some(entry_encoded.hash().into());
                    db.test_data.documents.push(document_id.clone().unwrap());
                }
            }
        }
    }
}

/// Helper method for publishing an operation encoded on an entry to a store.
pub async fn send_to_store(
    store: &SimplestStorageProvider,
    operation: &Operation,
    document_id: Option<&DocumentId>,
    key_pair: &KeyPair,
) -> (EntrySigned, PublishEntryResponse) {
    // Get an Author from the key_pair.
    let author = Author::try_from(key_pair.public_key().to_owned()).unwrap();

    // Get the next entry arguments for this author and the passed document id.
    let next_entry_args = store
        .get_entry_args(&EntryArgsRequest {
            public_key: author.clone(),
            document_id: document_id.cloned(),
        })
        .await
        .unwrap();

    // Construct the next entry.
    let next_entry = Entry::new(
        &next_entry_args.log_id,
        Some(operation),
        next_entry_args.skiplink.map(Hash::from).as_ref(),
        next_entry_args.backlink.map(Hash::from).as_ref(),
        &next_entry_args.seq_num,
    )
    .unwrap();

    // Encode both the entry and operation.
    let entry_encoded = sign_and_encode(&next_entry, key_pair).unwrap();
    let operation_encoded = OperationEncoded::try_from(operation).unwrap();

    // Publish the entry and get the next entry args.
    let publish_entry_request = PublishEntryRequest {
        entry: entry_encoded.clone(),
        operation: operation_encoded,
    };
    let publish_entry_response = store.publish_entry(&publish_entry_request).await.unwrap();

    // Set or unwrap the passed document_id.
    let document_id = if operation.is_create() {
        entry_encoded.hash().into()
    } else {
        document_id.unwrap().to_owned()
    };

    // Also insert the operation into the store.
    let verified_operation =
        VerifiedOperation::new(&author, &entry_encoded.hash().into(), operation).unwrap();
    store
        .insert_operation(&verified_operation, &document_id)
        .await
        .unwrap();

    (entry_encoded, publish_entry_response)
}

#[rstest]
#[async_std::test]
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

        if SKIPLINK_ENTRIES.contains(&(seq_num as u64)) {
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
