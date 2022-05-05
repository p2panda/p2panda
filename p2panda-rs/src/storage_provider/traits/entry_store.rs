use async_trait::async_trait;
use bamboo_rs_core_ed25519_yasmf::entry::is_lipmaa_required;

use crate::entry::LogId;
use crate::entry::SeqNum;
use crate::hash::Hash;
use crate::identity::Author;
use crate::schema::SchemaId;
use crate::storage_provider::errors::EntryStorageError;
use crate::storage_provider::traits::AsStorageEntry;

/// Trait which handles all storage actions relating to `Entry`.
///
/// This trait should be implemented on the root storage provider struct. It's definitions make up
/// the required methods for inserting and querying entries from storage.
#[async_trait]
pub trait EntryStore<StorageEntry: AsStorageEntry> {
    /// Get an entry by it's hash.
    async fn get_entry_by_hash(
        &self,
        hash: &Hash,
    ) -> Result<Option<StorageEntry>, EntryStorageError>;

    /// Get the backlink of a passed entry.
    ///
    /// Returns None if the entry has no backlink (it is a create), errors when a backlink
    /// was present but could not be found in the db.
    async fn try_get_backlink(
        &self,
        entry: &StorageEntry,
    ) -> Result<Option<StorageEntry>, EntryStorageError> {
        let backlink: Option<StorageEntry> = match entry.backlink_hash() {
            Some(backlink_hash) => Some(
                self.get_entry_by_hash(&backlink_hash)
                    .await?
                    .ok_or(EntryStorageError::BacklinkMissing(backlink_hash))?,
            ),
            None => None,
        };
        Ok(backlink)
    }

    /// Get the skiplink of a passed entry.
    ///
    /// Returns None if the passed entry has no skiplink, errors if a skiplink was present but could
    /// not be found in the db.
    async fn try_get_skiplink(
        &self,
        entry: &StorageEntry,
    ) -> Result<Option<StorageEntry>, EntryStorageError> {
        let skiplink: Option<StorageEntry> = match entry.skiplink_hash() {
            Some(skiplink_hash) => Some(
                self.get_entry_by_hash(&skiplink_hash)
                    .await?
                    .ok_or(EntryStorageError::SkiplinkMissing(skiplink_hash))?,
            ),
            None => None,
        };
        Ok(skiplink)
    }

    /// Insert an entry into storage.
    async fn insert_entry(&self, value: StorageEntry) -> Result<bool, EntryStorageError>;

    /// Returns entry at sequence position within an author's log.
    async fn entry_at_seq_num(
        &self,
        author: &Author,
        log_id: &LogId,
        seq_num: &SeqNum,
    ) -> Result<Option<StorageEntry>, EntryStorageError>;

    /// Returns the latest Bamboo entry of an author's log.
    async fn latest_entry(
        &self,
        author: &Author,
        log_id: &LogId,
    ) -> Result<Option<StorageEntry>, EntryStorageError>;

    /// Return vector of all entries of a given schema.
    async fn by_schema(&self, schema: &SchemaId) -> Result<Vec<StorageEntry>, EntryStorageError>;

    /// Determine skiplink entry hash ("lipmaa"-link) for entry in this log, return `None` when no
    /// skiplink is required for the next entry.
    async fn determine_skiplink(
        &self,
        entry: &StorageEntry,
    ) -> Result<Option<Hash>, EntryStorageError> {
        let next_seq_num = entry.seq_num().clone().next().unwrap();

        // Unwrap as we know that an skiplink exists as soon as previous entry is given
        let skiplink_seq_num = next_seq_num.skiplink_seq_num().unwrap();

        // Check if skiplink is required and return hash if so
        let entry_skiplink_hash = if is_lipmaa_required(next_seq_num.as_u64()) {
            let skiplink_entry = match self
                .entry_at_seq_num(&entry.author(), &entry.log_id(), &skiplink_seq_num)
                .await?
            {
                Some(entry) => Ok(entry),
                None => Err(EntryStorageError::ExpectedSkiplinkMissing),
            }?;
            Ok(Some(skiplink_entry.hash()))
        } else {
            Ok(None)
        };

        entry_skiplink_hash
    }
}

#[cfg(test)]
pub mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use rstest::rstest;

    use crate::entry::{sign_and_encode, Entry, EntrySigned, LogId, SeqNum};
    use crate::hash::Hash;
    use crate::identity::{Author, KeyPair};
    use crate::operation::{AsOperation, OperationEncoded};
    use crate::schema::SchemaId;
    use crate::storage_provider::errors::EntryStorageError;
    use crate::storage_provider::traits::test_utils::{
        test_db, SimplestStorageProvider, StorageEntry, SKIPLINK_ENTRIES,
    };
    use crate::storage_provider::traits::{AsStorageEntry, EntryStore};
    use crate::test_utils::fixtures::{
        entry, entry_signed_encoded, operation_encoded, random_key_pair, schema,
    };

    /// Implement `EntryStore` trait on `SimplestStorageProvider`
    #[async_trait]
    impl EntryStore<StorageEntry> for SimplestStorageProvider {
        /// Insert an entry into storage.
        async fn insert_entry(&self, entry: StorageEntry) -> Result<bool, EntryStorageError> {
            self.db_insert_entry(entry);
            Ok(true)
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
                entry.author() == *author
                    && entry.log_id() == *log_id
                    && entry.seq_num() == *seq_num
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

        /// Return vector of all entries of a given schema
        async fn by_schema(
            &self,
            schema: &SchemaId,
        ) -> Result<Vec<StorageEntry>, EntryStorageError> {
            let entries = self.entries.lock().unwrap();

            let entries: Vec<StorageEntry> = entries
                .iter()
                .filter(|entry| entry.operation().schema() == *schema)
                .map(|e| e.to_owned())
                .collect();

            Ok(entries)
        }
    }

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

    #[rstest]
    #[async_std::test]
    async fn get_entry_by_hash(test_db: SimplestStorageProvider) {
        let entries = test_db.entries.lock().unwrap().clone();

        assert_eq!(
            entries.get(0).cloned(),
            test_db.get_entry_by_hash(&entries[0].hash()).await.unwrap()
        );
        assert_eq!(
            entries.get(1).cloned(),
            test_db.get_entry_by_hash(&entries[1].hash()).await.unwrap()
        );
        assert_eq!(
            entries.get(2).cloned(),
            test_db.get_entry_by_hash(&entries[2].hash()).await.unwrap()
        );
    }

    #[rstest]
    #[async_std::test]
    async fn try_get_backlink(test_db: SimplestStorageProvider) {
        let entries = test_db.entries.lock().unwrap().clone();

        assert_eq!(
            entries.get(0).cloned(),
            test_db.try_get_backlink(&entries[1]).await.unwrap()
        );
        assert_eq!(
            entries.get(1).cloned(),
            test_db.try_get_backlink(&entries[2]).await.unwrap()
        );
        assert_eq!(
            entries.get(2).cloned(),
            test_db.try_get_backlink(&entries[3]).await.unwrap()
        );
    }

    #[rstest]
    #[async_std::test]
    async fn try_get_skiplink(test_db: SimplestStorageProvider) {
        let entries = test_db.entries.lock().unwrap().clone();

        assert_eq!(
            entries.get(0).cloned(),
            test_db.try_get_skiplink(&entries[3]).await.unwrap()
        );
        assert_eq!(
            entries.get(3).cloned(),
            test_db.try_get_skiplink(&entries[7]).await.unwrap()
        );
    }

    #[rstest]
    #[async_std::test]
    async fn can_determine_skiplink(test_db: SimplestStorageProvider) {
        let entries = test_db.entries.lock().unwrap().clone();
        for seq_num in 1..10 {
            let current_entry = entries.get(seq_num - 1).unwrap();
            let next_entry_skiplink = test_db.determine_skiplink(current_entry).await;
            assert!(next_entry_skiplink.is_ok());
            if SKIPLINK_ENTRIES.contains(&((seq_num + 1) as u64)) {
                assert!(next_entry_skiplink.unwrap().is_some());
            } else {
                assert!(next_entry_skiplink.unwrap().is_none())
            }
        }
    }

    #[rstest]
    #[async_std::test]
    async fn skiplink_does_not_exist(test_db: SimplestStorageProvider) {
        let entries = test_db.entries.lock().unwrap().clone();
        let logs = test_db.logs.lock().unwrap().clone();

        let log_entries_with_skiplink_missing = vec![
            entries.get(0).unwrap().clone(),
            entries.get(1).unwrap().clone(),
            entries.get(2).unwrap().clone(),
            entries.get(4).unwrap().clone(),
            entries.get(5).unwrap().clone(),
        ];

        let new_db = SimplestStorageProvider {
            logs: Arc::new(Mutex::new(logs)),
            entries: Arc::new(Mutex::new(log_entries_with_skiplink_missing)),
        };

        let error_response = new_db.determine_skiplink(entries.get(6).unwrap()).await;

        assert_eq!(
            format!("{}", error_response.unwrap_err()),
            "Could not find expected skiplink entry in database"
        )
    }
}
