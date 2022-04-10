// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::{TryFrom, TryInto};
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use rstest::{fixture, rstest};

use crate::document::DocumentId;
use crate::entry::{decode_entry, sign_and_encode, Entry, EntrySigned, LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::{Author, KeyPair};
use crate::operation::{Operation, OperationEncoded};
use crate::schema::SchemaId;
use crate::storage_provider::errors::{EntryStorageError, LogStorageError, ValidationError};
use crate::storage_provider::models::{EntryWithOperation, Log};
use crate::storage_provider::traits::{
    AsEntryArgsRequest, AsEntryArgsResponse, AsPublishEntryRequest, AsPublishEntryResponse,
    AsStorageEntry, AsStorageLog,
};
use crate::test_utils::fixtures::{
    create_operation, document_id, entry, random_key_pair, schema, update_operation,
};

/// The simplest storage provider. Used for tests in `entry_store`, `log_store` & `storage_provider`
pub struct SimplestStorageProvider {
    pub logs: Arc<Mutex<Vec<StorageLog>>>,
    pub entries: Arc<Mutex<Vec<StorageEntry>>>,
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
    fn new(log: Log) -> Self {
        // Concat all values
        let log_string = format!(
            "{}-{}-{}-{}",
            log.author().as_str(),
            log.schema().as_str(),
            log.document().as_str(),
            log.log_id().as_u64()
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

impl From<Log> for StorageLog {
    fn from(log: Log) -> Self {
        StorageLog::new(log)
    }
}

impl TryInto<Log> for StorageLog {
    type Error = LogStorageError;

    fn try_into(self) -> Result<Log, Self::Error> {
        Ok(Log {
            author: self.author(),
            log_id: self.id(),
            document: self.document_id(),
            schema: self.schema_id(),
        })
    }
}

/// A struct which represents an entry and operation pair in storage as a concatenated string.
#[derive(Debug, Clone, PartialEq)]
pub struct StorageEntry(String);

impl StorageEntry {
    fn entry_decoded(&self) -> Entry {
        // Unwrapping as validation occurs in `EntryWithOperation`.
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

/// Implement required `TryFrom` conversion trait.
impl From<EntryWithOperation> for StorageEntry {
    fn from(entry: EntryWithOperation) -> Self {
        let entry_string = format!(
            "{}-{}",
            entry.entry_signed().as_str(),
            entry.operation_encoded().as_str()
        );

        StorageEntry(entry_string)
    }
}

/// Implement required `TryInto` conversion trait.
impl TryInto<EntryWithOperation> for StorageEntry {
    type Error = ValidationError;

    fn try_into(self) -> Result<EntryWithOperation, Self::Error> {
        EntryWithOperation::new(&self.entry_signed(), &self.operation_encoded().unwrap())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PublishEntryRequest(pub EntrySigned, pub OperationEncoded);

impl TryInto<EntryWithOperation> for PublishEntryRequest {
    type Error = ValidationError;

    fn try_into(self) -> Result<EntryWithOperation, Self::Error> {
        EntryWithOperation::new(self.entry_signed(), self.operation_encoded())
    }
}

impl AsPublishEntryRequest for PublishEntryRequest {
    fn entry_signed(&self) -> &EntrySigned {
        &self.0
    }

    fn operation_encoded(&self) -> &OperationEncoded {
        &self.1
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

pub const SKIPLINK_ENTRIES: [u64; 2] = [4, 8];

#[fixture]
pub fn test_db(
    #[from(random_key_pair)] key_pair: KeyPair,
    create_operation: Operation,
    update_operation: Operation,
    schema: SchemaId,
    document_id: DocumentId,
) -> SimplestStorageProvider {
    // Initial empty entry vec.
    let mut db_entries: Vec<StorageEntry> = vec![];

    // Create a log vec with one log in it (which we create the entries for below)
    let author = Author::try_from(key_pair.public_key().to_owned()).unwrap();
    let db_logs: Vec<StorageLog> =
        vec![Log::new(&author, &schema, &document_id, &LogId::new(1)).into()];

    // Create and push a first entry containing a CREATE operation to the entries list
    let create_entry = entry(
        create_operation.clone(),
        SeqNum::new(1).unwrap(),
        None,
        None,
    );

    let encoded_entry = sign_and_encode(&create_entry, &key_pair).unwrap();
    let encoded_operation = OperationEncoded::try_from(&create_operation).unwrap();
    let storage_entry: StorageEntry = EntryWithOperation::new(&encoded_entry, &encoded_operation)
        .unwrap()
        .into();

    db_entries.push(storage_entry);

    // Create 9 more entries containing UPDATE operations with valid back- and skip- links
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

        let update_entry = entry(update_operation.clone(), seq_num, Some(backlink), skiplink);

        let encoded_entry = sign_and_encode(&update_entry, &key_pair).unwrap();
        let encoded_operation = OperationEncoded::try_from(&update_operation).unwrap();
        let storage_entry: StorageEntry =
            EntryWithOperation::new(&encoded_entry, &encoded_operation)
                .unwrap()
                .into();

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

#[rstest]
#[async_std::test]
async fn test_the_test_data_conversions(test_db: SimplestStorageProvider) {
    let storage_entry = test_db.entries.lock().unwrap().get(0).unwrap().clone();
    let storage_log = test_db.logs.lock().unwrap().get(0).unwrap().clone();

    let entry_with_operation: Result<EntryWithOperation, _> = storage_entry.clone().try_into();
    assert!(entry_with_operation.is_ok());

    let storage_entry_again = StorageEntry::try_from(entry_with_operation.unwrap());
    assert!(storage_entry_again.is_ok());

    let log: Result<Log, _> = storage_log.try_into();
    assert!(log.is_ok());

    let storage_log_again = StorageLog::try_from(log.unwrap());
    assert!(storage_log_again.is_ok());

    let publish_entry_request = PublishEntryRequest(
        storage_entry.entry_signed(),
        storage_entry.operation_encoded().unwrap(),
    );

    let entry_with_operation: Result<EntryWithOperation, _> = publish_entry_request.try_into();
    assert!(entry_with_operation.is_ok());
}
