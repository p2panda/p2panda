// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;
use lipmaa_link::get_lipmaa_links_back_to;

use crate::entry::{LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::Author;
use crate::operation::AsOperation;
use crate::schema::SchemaId;
use crate::storage_provider::errors::EntryStorageError;
use crate::storage_provider::traits::{AsStorageEntry, EntryStore};
use crate::test_utils::db::{SimplestStorageProvider, StorageEntry};

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

        Ok(entries.get(hash).cloned())
    }

    /// Returns entry at sequence position within an author's log.
    async fn get_entry_at_seq_num(
        &self,
        author: &Author,
        log_id: &LogId,
        seq_num: &SeqNum,
    ) -> Result<Option<StorageEntry>, EntryStorageError> {
        let entries = self.entries.lock().unwrap();

        let entry = entries.values().find(|entry| {
            entry.seq_num() == *seq_num && entry.author() == *author && entry.log_id() == *log_id
        });

        Ok(entry.cloned())
    }

    /// Returns the latest Bamboo entry of an author's log.
    async fn get_latest_entry(
        &self,
        author: &Author,
        log_id: &LogId,
    ) -> Result<Option<StorageEntry>, EntryStorageError> {
        let entries = self.entries.lock().unwrap();

        let latest_entry = entries
            .iter()
            .filter(|(_, entry)| entry.author() == *author && entry.log_id() == *log_id)
            .max_by_key(|(_, entry)| entry.seq_num().as_u64());

        Ok(latest_entry.map(|(_, entry)| entry).cloned())
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
            match self.get_entry_at_seq_num(author, log_id, &seq_num).await? {
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
    async fn get_entries_by_schema(
        &self,
        schema: &SchemaId,
    ) -> Result<Vec<StorageEntry>, EntryStorageError> {
        let entries = self.entries.lock().unwrap();

        let entries: Vec<StorageEntry> = entries
            .iter()
            .filter(|(_, entry)| entry.operation().schema() == *schema)
            .map(|(_, entry)| entry.to_owned())
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
            let entry = match self.get_entry_at_seq_num(author, log_id, &seq_num).await? {
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
    use std::convert::TryFrom;

    use rstest::rstest;

    use crate::entry::{sign_and_encode, Entry, EntrySigned, LogId, SeqNum};
    use crate::identity::{Author, KeyPair};
    use crate::operation::OperationEncoded;
    use crate::schema::SchemaId;
    use crate::storage_provider::traits::test_utils::{test_db, TestStore};
    use crate::storage_provider::traits::{AsStorageEntry, EntryStore};
    use crate::test_utils::db::{SimplestStorageProvider, StorageEntry};
    use crate::test_utils::fixtures::{
        entry, entry_signed_encoded, key_pair, operation_encoded, random_key_pair, schema,
    };

    #[rstest]
    #[async_std::test]
    async fn insert_get_entry(
        entry_signed_encoded: EntrySigned,
        operation_encoded: OperationEncoded,
    ) {
        // Instantiate a new store.
        let store = SimplestStorageProvider::default();

        let storage_entry = StorageEntry::new(&entry_signed_encoded, &operation_encoded).unwrap();

        // Insert an entry into the store.
        assert!(store.insert_entry(storage_entry.clone()).await.is_ok());

        // Get an entry at a specific seq number from an authors log.
        let entry_at_seq_num = store
            .get_entry_at_seq_num(
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
        let store = SimplestStorageProvider::default();

        let storage_entry = StorageEntry::new(&entry_signed_encoded, &operation_encoded).unwrap();

        // Before an entry is inserted the latest entry should be none.
        assert!(store
            .get_latest_entry(&storage_entry.author(), &LogId::default())
            .await
            .unwrap()
            .is_none());

        // Insert an entry into the store.
        assert!(store.insert_entry(storage_entry.clone()).await.is_ok());

        assert_eq!(
            store
                .get_latest_entry(&storage_entry.author(), &LogId::default())
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
        let store = SimplestStorageProvider::default();

        let author_1_entry = sign_and_encode(&entry, &key_pair_1).unwrap();
        let author_2_entry = sign_and_encode(&entry, &key_pair_2).unwrap();
        let author_1_entry = StorageEntry::new(&author_1_entry, &operation_encoded).unwrap();
        let author_2_entry = StorageEntry::new(&author_2_entry, &operation_encoded).unwrap();

        // Before an entry with this schema is inserted this method should return an empty array.
        assert!(store
            .get_entries_by_schema(&schema)
            .await
            .unwrap()
            .is_empty());

        // Insert two entries into the store.
        store.insert_entry(author_1_entry).await.unwrap();
        store.insert_entry(author_2_entry).await.unwrap();

        assert_eq!(store.get_entries_by_schema(&schema).await.unwrap().len(), 2);
    }

    #[rstest]
    #[async_std::test]
    async fn get_entry_by_hash(
        #[from(test_db)]
        #[with(3, 1, 1)]
        #[future]
        db: TestStore,
    ) {
        let db = db.await;
        let entries = db.store.entries.lock().unwrap().clone();

        let entry_one = entries
            .values()
            .find(|entry| entry.seq_num().as_u64() == 1)
            .unwrap();

        let entry_two = entries
            .values()
            .find(|entry| entry.seq_num().as_u64() == 2)
            .unwrap();

        let entry_three = entries
            .values()
            .find(|entry| entry.seq_num().as_u64() == 2)
            .unwrap();

        assert_eq!(
            *entry_one,
            db.store
                .get_entry_by_hash(&entry_one.hash())
                .await
                .unwrap()
                .unwrap()
        );
        assert_eq!(
            *entry_two,
            db.store
                .get_entry_by_hash(&entry_two.hash())
                .await
                .unwrap()
                .unwrap()
        );
        assert_eq!(
            *entry_three,
            db.store
                .get_entry_by_hash(&entry_three.hash())
                .await
                .unwrap()
                .unwrap()
        );
    }

    #[rstest]
    #[async_std::test]
    async fn get_n_entries(
        key_pair: KeyPair,
        #[from(test_db)]
        #[with(16, 1, 1)]
        #[future]
        db: TestStore,
    ) {
        let db = db.await;
        let author = Author::try_from(*key_pair.public_key()).unwrap();
        let log_id = LogId::default();

        let five_entries = db
            .store
            .get_paginated_log_entries(&author, &log_id, &SeqNum::new(1).unwrap(), 5)
            .await
            .unwrap();
        assert_eq!(five_entries.len(), 5);

        let end_of_log_reached = db
            .store
            .get_paginated_log_entries(&author, &log_id, &SeqNum::new(1).unwrap(), 1000)
            .await
            .unwrap();
        assert_eq!(end_of_log_reached.len(), 16);

        let first_entry_not_found = db
            .store
            .get_paginated_log_entries(&author, &log_id, &SeqNum::new(10000).unwrap(), 1)
            .await
            .unwrap();
        assert!(first_entry_not_found.is_empty());
    }

    #[rstest]
    #[async_std::test]
    async fn get_cert_pool(
        key_pair: KeyPair,
        #[from(test_db)]
        #[with(17, 1, 1)]
        #[future]
        db: TestStore,
    ) {
        let db = db.await;
        let author = Author::try_from(*key_pair.public_key()).unwrap();
        let log_id = LogId::default();

        let cert_pool = db
            .store
            .get_certificate_pool(&author, &log_id, &SeqNum::new(16).unwrap())
            .await
            .unwrap();

        let seq_nums: Vec<u64> = cert_pool
            .iter()
            .map(|entry| entry.seq_num().as_u64())
            .collect();

        assert_eq!(seq_nums, vec![15, 14, 13, 4, 1]);
    }
}
