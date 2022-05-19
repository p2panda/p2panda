// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;
use lipmaa_link::get_lipmaa_links_back_to;

use crate::entry::{decode_entry, Entry, EntrySigned, LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::Author;
use crate::operation::{AsOperation, Operation, OperationEncoded};
use crate::schema::SchemaId;
use crate::storage_provider::entry::{AsStorageEntry, EntryStorageError, EntryStore};
use crate::storage_provider::errors::ValidationError;
use crate::storage_provider::test_provider::SimplestStorageProvider;
use crate::Validate;

/// A struct which represents an entry and operation pair in storage as a concatenated string.
#[derive(Debug, Clone, PartialEq)]
pub struct StorageEntry(String);

impl StorageEntry {
    pub fn entry_decoded(&self) -> Entry {
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

/// Implement `EntryStore` trait on `SimplestStorageProvider`
#[async_trait]
impl EntryStore<StorageEntry> for SimplestStorageProvider {
    /// Insert an entry into storage.
    async fn insert_entry(&self, entry: StorageEntry) -> Result<(), EntryStorageError> {
        self.db_insert_entry(entry);
        Ok(())
    }

    /// Get an entry by it's hash id.
    async fn get_entry_by_hash(
        &self,
        hash: &Hash,
    ) -> Result<Option<StorageEntry>, EntryStorageError> {
        let entries = self.entries.lock().unwrap();

        let entry = entries.iter().find(|entry| entry.hash() == *hash);

        Ok(entry.cloned())
    }

    /// Returns entry at sequence position within an author's log.
    async fn entry_at_seq_num(
        &self,
        author: &Author,
        log_id: &LogId,
        seq_num: &SeqNum,
    ) -> Result<Option<StorageEntry>, EntryStorageError> {
        let entries = self.entries.lock().unwrap();

        let entry = entries.iter().find(|entry| {
            entry.author() == *author && entry.log_id() == *log_id && entry.seq_num() == *seq_num
        });

        Ok(entry.cloned())
    }

    /// Returns the latest Bamboo entry of an author's log.
    async fn latest_entry(
        &self,
        author: &Author,
        log_id: &LogId,
    ) -> Result<Option<StorageEntry>, EntryStorageError> {
        let entries = self.entries.lock().unwrap();

        let latest_entry = entries
            .iter()
            .filter(|entry| entry.author() == *author && entry.log_id() == *log_id)
            .max_by_key(|entry| entry.seq_num().as_u64());

        Ok(latest_entry.cloned())
    }

    /// Returns the given range of log entries.
    async fn get_paginated_log_entries(
        &self,
        author: &Author,
        log_id: &LogId,
        seq_num: &SeqNum,
        max_number_of_entries: usize,
    ) -> Result<Vec<StorageEntry>, EntryStorageError> {
        let mut entries: Vec<StorageEntry> = Vec::new();
        let mut seq_num = *seq_num;

        while entries.len() < max_number_of_entries {
            match self.entry_at_seq_num(author, log_id, &seq_num).await? {
                Some(next_entry) => entries.push(next_entry),
                None => break,
            };
            match seq_num.next() {
                Some(next_seq_num) => seq_num = next_seq_num,
                None => break,
            };
        }
        Ok(entries)
    }

    /// Return vector of all entries of a given schema
    async fn by_schema(&self, schema: &SchemaId) -> Result<Vec<StorageEntry>, EntryStorageError> {
        let entries = self.entries.lock().unwrap();

        let entries: Vec<StorageEntry> = entries
            .iter()
            .filter(|entry| entry.operation().schema() == *schema)
            .map(|e| e.to_owned())
            .collect();

        Ok(entries)
    }

    async fn get_certificate_pool(
        &self,
        author: &Author,
        log_id: &LogId,
        initial_seq_num: &SeqNum,
    ) -> Result<Vec<StorageEntry>, EntryStorageError> {
        let seq_num = initial_seq_num.as_u64();
        let cert_pool_seq_nums: Vec<SeqNum> = get_lipmaa_links_back_to(seq_num, 1)
            .iter()
            // Unwrapping as we know this is a valid sequence number
            .map(|seq_num| SeqNum::new(*seq_num).unwrap())
            .collect();
        let mut cert_pool: Vec<StorageEntry> = Vec::new();

        for seq_num in cert_pool_seq_nums {
            let entry = match self.entry_at_seq_num(author, log_id, &seq_num).await? {
                Some(entry) => Ok(entry),
                None => Err(EntryStorageError::CertPoolEntryMissing(seq_num.as_u64())),
            }?;
            cert_pool.push(entry);
        }

        Ok(cert_pool)
    }
}

#[cfg(test)]
pub mod tests {
    use std::sync::{Arc, Mutex};

    use rstest::rstest;

    use crate::entry::{sign_and_encode, Entry, EntrySigned, LogId};
    use crate::identity::KeyPair;
    use crate::operation::OperationEncoded;
    use crate::schema::SchemaId;
    use crate::storage_provider::entry::{AsStorageEntry, EntryStore};
    use crate::storage_provider::test_provider::{SimplestStorageProvider, StorageEntry};
    use crate::test_utils::fixtures::{
        entry, entry_signed_encoded, operation_encoded, random_key_pair, schema,
    };

    #[rstest]
    #[async_std::test]
    async fn insert_get_entry(
        entry_signed_encoded: EntrySigned,
        operation_encoded: OperationEncoded,
    ) {
        // Instantiate a new store.
        let store = SimplestStorageProvider {
            logs: Arc::new(Mutex::new(Vec::new())),
            entries: Arc::new(Mutex::new(Vec::new())),
        };

        let storage_entry = StorageEntry::new(&entry_signed_encoded, &operation_encoded).unwrap();

        // Insert an entry into the store.
        assert!(store.insert_entry(storage_entry.clone()).await.is_ok());

        // Get an entry at a specific seq number from an authors log.
        let entry_at_seq_num = store
            .entry_at_seq_num(
                &storage_entry.author(),
                &storage_entry.log_id(),
                &storage_entry.seq_num(),
            )
            .await;

        assert!(entry_at_seq_num.is_ok());
        assert_eq!(entry_at_seq_num.unwrap().unwrap(), storage_entry)
    }

    #[rstest]
    #[async_std::test]
    async fn get_latest_entry(
        entry_signed_encoded: EntrySigned,
        operation_encoded: OperationEncoded,
    ) {
        // Instantiate a new store.
        let store = SimplestStorageProvider {
            logs: Arc::new(Mutex::new(Vec::new())),
            entries: Arc::new(Mutex::new(Vec::new())),
        };

        let storage_entry = StorageEntry::new(&entry_signed_encoded, &operation_encoded).unwrap();

        // Before an entry is inserted the latest entry should be none.
        assert!(store
            .latest_entry(&storage_entry.author(), &LogId::default())
            .await
            .unwrap()
            .is_none());

        // Insert an entry into the store.
        assert!(store.insert_entry(storage_entry.clone()).await.is_ok());

        assert_eq!(
            store
                .latest_entry(&storage_entry.author(), &LogId::default())
                .await
                .unwrap()
                .unwrap(),
            storage_entry
        );
    }

    #[rstest]
    #[async_std::test]
    async fn get_by_schema(
        #[from(random_key_pair)] key_pair_1: KeyPair,
        #[from(random_key_pair)] key_pair_2: KeyPair,
        entry: Entry,
        operation_encoded: OperationEncoded,
        schema: SchemaId,
    ) {
        // Instantiate a new store.
        let store = SimplestStorageProvider {
            logs: Arc::new(Mutex::new(Vec::new())),
            entries: Arc::new(Mutex::new(Vec::new())),
        };

        let author_1_entry = sign_and_encode(&entry, &key_pair_1).unwrap();
        let author_2_entry = sign_and_encode(&entry, &key_pair_2).unwrap();
        let author_1_entry = StorageEntry::new(&author_1_entry, &operation_encoded).unwrap();
        let author_2_entry = StorageEntry::new(&author_2_entry, &operation_encoded).unwrap();

        // Before an entry with this schema is inserted this method should return an empty array.
        assert!(store.by_schema(&schema).await.unwrap().is_empty());

        // Insert two entries into the store.
        store.insert_entry(author_1_entry).await.unwrap();
        store.insert_entry(author_2_entry).await.unwrap();

        assert_eq!(store.by_schema(&schema).await.unwrap().len(), 2);
    }
}
