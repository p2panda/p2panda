// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;
use log::debug;

use crate::entry::traits::{AsEncodedEntry, AsEntry};
use crate::entry::{EncodedEntry, Entry, LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::PublicKey;
use crate::operation::EncodedOperation;
use crate::storage_provider::error::EntryStorageError;
use crate::storage_provider::traits::EntryStore;
use crate::test_utils::memory_store::{MemoryStore, StorageEntry};

/// Implement `EntryStore` trait on `MemoryStore`
#[async_trait]
impl EntryStore for MemoryStore {
    type Entry = StorageEntry;

    /// Insert an `Entry` to the store in it's encoded and decoded form. Optionally also store it's encoded
    /// operation.
    async fn insert_entry(
        &self,
        entry: &Entry,
        encoded_entry: &EncodedEntry,
        operation: Option<&EncodedOperation>,
    ) -> Result<(), EntryStorageError> {
        debug!("Inserting entry: {} into store", encoded_entry.hash());

        let storage_entry = StorageEntry {
            entry: entry.to_owned(),
            encoded_entry: encoded_entry.to_owned(),
            payload: operation.cloned(),
        };

        let mut entries = self.entries.lock().unwrap();
        entries.insert(encoded_entry.hash(), storage_entry);
        Ok(())
    }

    /// Get an `Entry` by it's `Hash`.
    async fn get_entry(&self, hash: &Hash) -> Result<Option<StorageEntry>, EntryStorageError> {
        let entries = self.entries.lock().unwrap();

        Ok(entries.get(hash).cloned())
    }

    /// Get an `Entry` at sequence position within a `PublicKey`'s log.
    async fn get_entry_at_seq_num(
        &self,
        public_key: &PublicKey,
        log_id: &LogId,
        seq_num: &SeqNum,
    ) -> Result<Option<StorageEntry>, EntryStorageError> {
        let entries = self.entries.lock().unwrap();

        let entry = entries.values().find(|entry| {
            entry.seq_num() == seq_num
                && entry.public_key() == public_key
                && entry.log_id() == log_id
        });

        Ok(entry.cloned())
    }

    /// Get the latest `Entry` of `PublicKey`'s log.
    async fn get_latest_entry(
        &self,
        public_key: &PublicKey,
        log_id: &LogId,
    ) -> Result<Option<StorageEntry>, EntryStorageError> {
        let entries = self.entries.lock().unwrap();

        let latest_entry = entries
            .iter()
            .filter(|(_, entry)| entry.public_key() == public_key && entry.log_id() == log_id)
            .max_by_key(|(_, entry)| entry.seq_num().as_u64());

        Ok(latest_entry.map(|(_, entry)| entry).cloned())
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::entry::decode::decode_entry;
    use crate::entry::traits::{AsEncodedEntry, AsEntry};
    use crate::entry::EncodedEntry;
    use crate::storage_provider::traits::EntryStore;
    use crate::test_utils::fixtures::{encoded_entry, populate_store_config};
    use crate::test_utils::memory_store::helpers::{populate_store, PopulateStoreConfig};
    use crate::test_utils::memory_store::MemoryStore;

    #[rstest]
    #[tokio::test]
    async fn insert_get_entry(encoded_entry: EncodedEntry) {
        // Instantiate a new store.
        let store = MemoryStore::default();

        // Insert an entry into the store.
        let entry = decode_entry(&encoded_entry).unwrap();
        assert!(store
            .insert_entry(&entry, &encoded_entry, None)
            .await
            .is_ok());

        // Get an entry at a specific seq number from an authors log.
        let entry_at_seq_num = store
            .get_entry_at_seq_num(entry.public_key(), entry.log_id(), entry.seq_num())
            .await;

        assert!(entry_at_seq_num.is_ok());

        let entry_at_seq_num = entry_at_seq_num.unwrap().unwrap();

        assert_eq!(entry_at_seq_num.seq_num(), entry.seq_num());
        assert_eq!(entry_at_seq_num.hash(), encoded_entry.hash());
    }

    #[rstest]
    #[tokio::test]
    async fn get_latest_entry(encoded_entry: EncodedEntry) {
        // Instantiate a new store.
        let store = MemoryStore::default();
        let entry = decode_entry(&encoded_entry).unwrap();

        // Before an entry is inserted the latest entry should be none.
        assert!(store
            .get_latest_entry(entry.public_key(), entry.log_id())
            .await
            .unwrap()
            .is_none());

        // Insert an entry into the store.
        assert!(store
            .insert_entry(&entry, &encoded_entry, None)
            .await
            .is_ok());

        let fetched_entry = store
            .get_latest_entry(entry.public_key(), entry.log_id())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(fetched_entry.hash(), encoded_entry.hash());
    }

    #[rstest]
    #[tokio::test]
    async fn get_entry(
        #[from(populate_store_config)]
        #[with(3, 1, 1)]
        config: PopulateStoreConfig,
    ) {
        let store = MemoryStore::default();
        populate_store(&store, &config).await;
        let entries = store.entries.lock().unwrap().clone();

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
            store.get_entry(&entry_one.hash()).await.unwrap().unwrap()
        );
        assert_eq!(
            *entry_two,
            store.get_entry(&entry_two.hash()).await.unwrap().unwrap()
        );
        assert_eq!(
            *entry_three,
            store.get_entry(&entry_three.hash()).await.unwrap().unwrap()
        );
    }
}
