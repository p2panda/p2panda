// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use rstest::{fixture, rstest};

use crate::document::DocumentId;
use crate::entry::{decode_entry, sign_and_encode, Entry, EntrySigned, LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::{Author, KeyPair};
use crate::operation::{
    AsVerifiedOperation, Operation, OperationEncoded, OperationValue, VerifiedOperation,
};
use crate::schema::SchemaId;
use crate::storage_provider::errors::{EntryStorageError, ValidationError};
use crate::storage_provider::traits::{
    AsEntryArgsRequest, AsEntryArgsResponse, AsPublishEntryRequest, AsPublishEntryResponse,
    AsStorageEntry, AsStorageLog,
};
use crate::test_utils::constants::{default_fields, DEFAULT_PRIVATE_KEY, TEST_SCHEMA_ID};
use crate::test_utils::fixtures::{create_operation, delete_operation, update_operation};
use crate::Validate;

use super::{EntryStore, LogStore, OperationStore, StorageProvider};

/// The simplest storage provider. Used for tests in `entry_store`, `log_store` & `storage_provider`
#[derive(Default)]
pub struct SimplestStorageProvider {
    pub logs: Arc<Mutex<Vec<StorageLog>>>,
    pub entries: Arc<Mutex<Vec<StorageEntry>>>,
    pub operations: Arc<Mutex<Vec<(DocumentId, VerifiedOperation)>>>,
}

impl SimplestStorageProvider {
    pub fn db_insert_entry(&self, entry: StorageEntry) {
        let mut entries = self.entries.lock().unwrap();
        entries.push(entry);
        // Remove duplicate entries.
        entries.dedup();
    }

    pub fn db_insert_log(&self, log: StorageLog) {
        let mut logs = self.logs.lock().unwrap();
        logs.push(log);
        // Remove duplicate logs.
        logs.dedup();
    }
}

/// A log entry represented as a concatenated string of `"{author}-{schema}-{document_id}-{log_id}"`
#[derive(Debug, Clone, PartialEq)]
pub struct StorageLog(String);

/// Implement `AsStorageLog` trait for our `StorageLog` struct
impl AsStorageLog for StorageLog {
    fn new(author: &Author, schema: &SchemaId, document: &DocumentId, log_id: &LogId) -> Self {
        // Concat all values
        let log_string = format!(
            "{}-{}-{}-{}",
            author.as_str(),
            schema.as_str(),
            document.as_str(),
            log_id.as_u64()
        );

        Self(log_string)
    }

    fn author(&self) -> Author {
        let params: Vec<&str> = self.0.split('-').collect();
        Author::new(params[0]).unwrap()
    }

    fn schema_id(&self) -> SchemaId {
        let params: Vec<&str> = self.0.split('-').collect();
        SchemaId::from_str(params[1]).unwrap()
    }

    fn document_id(&self) -> DocumentId {
        let params: Vec<&str> = self.0.split('-').collect();
        DocumentId::from_str(params[2]).unwrap()
    }

    fn id(&self) -> LogId {
        let params: Vec<&str> = self.0.split('-').collect();
        LogId::from_str(params[3]).unwrap()
    }
}

/// A struct which represents an entry and operation pair in storage as a concatenated string.
#[derive(Debug, Clone, PartialEq)]
pub struct StorageEntry(String);

impl StorageEntry {
    fn entry_decoded(&self) -> Entry {
        // Unwrapping as validation occurs in constructor.
        decode_entry(&self.entry_signed(), self.operation_encoded().as_ref()).unwrap()
    }

    pub fn entry_signed(&self) -> EntrySigned {
        let params: Vec<&str> = self.0.split('-').collect();
        EntrySigned::new(params[0]).unwrap()
    }

    pub fn operation_encoded(&self) -> Option<OperationEncoded> {
        let params: Vec<&str> = self.0.split('-').collect();
        Some(OperationEncoded::new(params[1]).unwrap())
    }
}

/// Implement `AsStorageEntry` trait for `StorageEntry`
impl AsStorageEntry for StorageEntry {
    type AsStorageEntryError = EntryStorageError;

    fn new(
        entry: &EntrySigned,
        operation: &OperationEncoded,
    ) -> Result<Self, Self::AsStorageEntryError> {
        let entry_string = format!("{}-{}", entry.as_str(), operation.as_str());
        let storage_entry = Self(entry_string);
        storage_entry.validate()?;
        Ok(storage_entry)
    }

    fn author(&self) -> Author {
        self.entry_signed().author()
    }

    fn hash(&self) -> Hash {
        self.entry_signed().hash()
    }

    fn entry_bytes(&self) -> Vec<u8> {
        self.entry_signed().to_bytes()
    }

    fn backlink_hash(&self) -> Option<Hash> {
        self.entry_decoded().backlink_hash().cloned()
    }

    fn skiplink_hash(&self) -> Option<Hash> {
        self.entry_decoded().skiplink_hash().cloned()
    }

    fn seq_num(&self) -> SeqNum {
        *self.entry_decoded().seq_num()
    }

    fn log_id(&self) -> LogId {
        *self.entry_decoded().log_id()
    }

    fn operation(&self) -> Operation {
        let operation_encoded = self.operation_encoded().unwrap();
        Operation::from(&operation_encoded)
    }
}

impl Validate for StorageEntry {
    type Error = ValidationError;

    fn validate(&self) -> Result<(), Self::Error> {
        self.entry_signed().validate()?;
        if let Some(operation) = self.operation_encoded() {
            operation.validate()?;
        }
        decode_entry(&self.entry_signed(), self.operation_encoded().as_ref())?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PublishEntryRequest(pub EntrySigned, pub OperationEncoded);

impl AsPublishEntryRequest for PublishEntryRequest {
    fn entry_signed(&self) -> &EntrySigned {
        &self.0
    }

    fn operation_encoded(&self) -> &OperationEncoded {
        &self.1
    }
}

impl Validate for PublishEntryRequest {
    type Error = ValidationError;

    fn validate(&self) -> Result<(), Self::Error> {
        self.entry_signed().validate()?;
        self.operation_encoded().validate()?;
        decode_entry(self.entry_signed(), Some(self.operation_encoded()))?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PublishEntryResponse {
    entry_hash_backlink: Option<Hash>,
    entry_hash_skiplink: Option<Hash>,
    seq_num: SeqNum,
    log_id: LogId,
}

impl AsPublishEntryResponse for PublishEntryResponse {
    /// Just the constructor method is defined here as all we need this trait for
    /// is constructing entry args to be returned from the default trait methods.
    fn new(
        entry_hash_backlink: Option<Hash>,
        entry_hash_skiplink: Option<Hash>,
        seq_num: SeqNum,
        log_id: LogId,
    ) -> Self {
        Self {
            entry_hash_backlink,
            entry_hash_skiplink,
            seq_num,
            log_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntryArgsResponse {
    entry_hash_backlink: Option<Hash>,
    entry_hash_skiplink: Option<Hash>,
    seq_num: SeqNum,
    log_id: LogId,
}

impl AsEntryArgsResponse for EntryArgsResponse {
    /// Just the constructor method is defined here as all we need this trait for
    /// is constructing entry args to be returned from the default trait methods.
    fn new(
        entry_hash_backlink: Option<Hash>,
        entry_hash_skiplink: Option<Hash>,
        seq_num: SeqNum,
        log_id: LogId,
    ) -> Self {
        Self {
            entry_hash_backlink,
            entry_hash_skiplink,
            seq_num,
            log_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntryArgsRequest {
    pub author: Author,
    pub document: Option<DocumentId>,
}

impl AsEntryArgsRequest for EntryArgsRequest {
    fn author(&self) -> &Author {
        &self.author
    }
    fn document_id(&self) -> &Option<DocumentId> {
        &self.document
    }
}

impl Validate for EntryArgsRequest {
    type Error = ValidationError;

    fn validate(&self) -> Result<(), Self::Error> {
        // Validate `author` request parameter
        self.author().validate()?;

        // Validate `document` request parameter when it is set
        match self.document_id() {
            None => (),
            Some(doc) => {
                doc.validate()?;
            }
        };
        Ok(())
    }
}

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
