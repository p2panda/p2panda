use async_trait::async_trait;
use bamboo_rs_core_ed25519_yasmf::entry::is_lipmaa_required;
use mockall::automock;

use crate::entry::LogId;
use crate::entry::SeqNum;
use crate::hash::Hash;
use crate::identity::Author;
use crate::schema::SchemaId;
use crate::storage_provider::entry::{AsStorageEntry, EntryStorageError};

/// Trait which handles all storage actions relating to `Entry`.
///
/// This trait should be implemented on the root storage provider struct. It's definitions make up
/// the required methods for inserting and querying entries from storage.
#[automock]
#[async_trait]
pub trait EntryStore<StorageEntry: AsStorageEntry> {
    /// Insert an entry into storage.
    ///
    /// Returns an error if a fatal storage error occured.
    async fn insert_entry(&self, value: StorageEntry) -> Result<(), EntryStorageError>;

    /// Get an entry at sequence position within an author's log.
    ///
    /// Returns a result containing an entry wrapped in an option. If no entry could
    /// be found at this author - log - seq number location then None is returned.
    /// Errors when a fatal storage error occurs.
    async fn entry_at_seq_num(
        &self,
        author: &Author,
        log_id: &LogId,
        seq_num: &SeqNum,
    ) -> Result<Option<StorageEntry>, EntryStorageError>;

    /// Get an entry by it's hash.
    async fn get_entry_by_hash(
        &self,
        hash: &Hash,
    ) -> Result<Option<StorageEntry>, EntryStorageError>;

    /// Retrieve and verify the backlink of a passed entry.
    ///
    /// Fetches the expected backlink for the passed entry (based on SeqNum, Author & LogId), if
    /// it is found it verifies its hash against the backlink entry hash encoded in the passed
    /// entry.
    ///
    /// If either the expected backlink is not found in storage, or it doesn't match the one encoded in
    /// the passed entry, then an error is returned. If the passed entry doesn't require a backlink
    /// (it is at sequence number 1) then `None` is returned.
    ///
    /// If the backlink is retrieved and validated against the encoded entries backlink successfully
    /// the backlink entry is returned.
    async fn try_get_backlink(
        &self,
        entry: &StorageEntry,
    ) -> Result<Option<StorageEntry>, EntryStorageError> {
        if entry.seq_num().is_first() {
            return Ok(None);
        };

        // Unwrap as we know this isn't the first sequence number because of the above condition
        let backlink_seq_num = SeqNum::new(entry.seq_num().as_u64() - 1).unwrap();
        let expected_backlink = self
            .entry_at_seq_num(&entry.author(), &entry.log_id(), &backlink_seq_num)
            .await?
            .ok_or_else(|| EntryStorageError::ExpectedBacklinkMissing(entry.hash()))?;

        // compare the expected backlink hash and the stated backlink hash
        if expected_backlink.hash() != entry.backlink_hash().unwrap() {
            return Err(EntryStorageError::InvalidBacklinkPassed(entry.hash()));
        }
        Ok(Some(expected_backlink))
    }

    /// Retrieve and verify the skiplink of a passed entry.
    ///
    /// Fetches the expected skiplink for the passed entry (based on SeqNum, Author & LogId), if
    /// it is found it verifies its hash against the skiplink entry hash encoded in the passed
    /// entry.
    ///
    /// If either the expected skiplink is not found in storage, or it doesn't match the one
    /// encoded in the passed entry, then an error is returned. If no skiplink is required for an
    /// entry at this seq num, and it wasn't encoded with one, then `None` is returned.
    ///
    /// If the skiplink is retrieved and validated against the encoded entries skiplink
    /// successfully the skiplink entry is returned.
    async fn try_get_skiplink(
        &self,
        entry: &StorageEntry,
    ) -> Result<Option<StorageEntry>, EntryStorageError> {
        // If a skiplink isn't required and it wasn't provided, return already now
        if !is_lipmaa_required(entry.seq_num().as_u64()) && entry.skiplink_hash().is_none() {
            return Ok(None);
        };

        // Derive the expected skiplink seq number from this entries sequence number
        let expected_skiplink = match entry.seq_num().skiplink_seq_num() {
            // Retrieve the expected skiplink from the database
            Some(seq_num) => {
                let expected_skiplink_entry = self
                    .entry_at_seq_num(&entry.author(), &entry.log_id(), &seq_num)
                    .await?
                    .ok_or_else(|| EntryStorageError::ExpectedSkiplinkMissing(entry.hash()))?;
                Some(expected_skiplink_entry)
            }
            // Or if there is no skiplink for entries at this sequence number return None
            None => None,
        };

        // compare the expected skiplink hash and the stated skiplink hash
        if expected_skiplink.clone().map(|entry| entry.hash()) != entry.skiplink_hash() {
            return Err(EntryStorageError::InvalidSkiplinkPassed(entry.hash()));
        }
        Ok(expected_skiplink)
    }
    /// Get the latest Bamboo entry of an author's log.
    ///
    /// Returns a result containing an entry wrapped in an option. If no log was
    /// could be found at this author - log location then None is returned.
    /// Errors when a fatal storage error occurs.
    async fn latest_entry(
        &self,
        author: &Author,
        log_id: &LogId,
    ) -> Result<Option<StorageEntry>, EntryStorageError>;

    /// Get a vector of all entries of a given schema.
    ///
    /// Returns a result containing vector of entries wrapped in an option.
    /// If no schema with this id could be found then None is returned.
    /// Errors when a fatal storage error occurs.
    async fn by_schema(&self, schema: &SchemaId) -> Result<Vec<StorageEntry>, EntryStorageError>;

    /// Get all entries of a log from a specified sequence number up to passed max number of entries.
    ///
    /// Returns a vector of entries the length of which will not be greater than the max number
    /// passed into the method. Fewer may be returned if the end of the log is reached.
    async fn get_paginated_log_entries(
        &self,
        author: &Author,
        log_id: &LogId,
        seq_num: &SeqNum,
        max_number_of_entries: usize,
    ) -> Result<Vec<StorageEntry>, EntryStorageError>;

    /// Determine skiplink entry hash ("lipmaa"-link) for the entry following the one passed, returns
    /// `None` when no skiplink is required for the next entry.
    async fn determine_next_skiplink(
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
                None => Err(EntryStorageError::ExpectedNextSkiplinkMissing),
            }?;
            Ok(Some(skiplink_entry.hash()))
        } else {
            Ok(None)
        };

        entry_skiplink_hash
    }

    /// Get all entries which make up the certificate pool for the given entry.
    ///
    /// Returns a result containing vector of entries wrapped in an option. If no entry
    /// could be found at this author - log - seq number location then an error is
    /// returned.
    async fn get_certificate_pool(
        &self,
        author_id: &Author,
        log_id: &LogId,
        seq_num: &SeqNum,
    ) -> Result<Vec<StorageEntry>, EntryStorageError>;
}

#[cfg(test)]
pub mod tests {
    use std::convert::TryFrom;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use lipmaa_link::get_lipmaa_links_back_to;
    use rstest::rstest;

    use crate::entry::{sign_and_encode, Entry, EntrySigned, LogId, SeqNum};
    use crate::hash::Hash;
    use crate::identity::{Author, KeyPair};
    use crate::operation::{AsOperation, Operation, OperationEncoded};
    use crate::schema::SchemaId;
    use crate::storage_provider::entry::{AsStorageEntry, EntryStorageError, EntryStore};
    use crate::storage_provider::test_utils::{
        test_db, SimplestStorageProvider, StorageEntry, SKIPLINK_ENTRIES,
    };
    use crate::test_utils::fixtures::{
        entry, entry_signed_encoded, key_pair, operation_encoded, random_key_pair, schema,
    };

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
    async fn try_get_backlink(
        #[values[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14 ,15 ,16]] seq_num: usize,
        test_db: SimplestStorageProvider,
    ) {
        let entries = test_db.entries.lock().unwrap().clone();

        let backlink = if seq_num < 2 {
            None
        } else {
            entries.get(seq_num - 2).cloned()
        };

        assert_eq!(
            backlink,
            test_db
                .try_get_backlink(&entries[seq_num - 1])
                .await
                .unwrap()
        );
    }

    #[rstest]
    #[async_std::test]
    async fn try_get_backlink_entry_missing(test_db: SimplestStorageProvider) {
        let entries = test_db.entries.lock().unwrap().clone();

        // Get the entry with seq number 2
        let entry_at_seq_num_two = entries.get(1).unwrap();

        {
            // Remove the entry at seq_num 1, which is the expected backlink for the above seq num 2 entry
            test_db.entries.lock().unwrap().remove(0);
        }
        assert_eq!(
            test_db
                .try_get_backlink(entry_at_seq_num_two)
                .await
                .unwrap_err()
                .to_string(),
            format!(
                "Could not find expected backlink in database for entry with id: {}",
                entry_at_seq_num_two.hash()
            )
        );
    }

    #[rstest]
    #[async_std::test]
    async fn try_get_backlink_invalid_skiplink(
        key_pair: KeyPair,
        operation_encoded: OperationEncoded,
        test_db: SimplestStorageProvider,
    ) {
        let entries = test_db.entries.lock().unwrap().clone();

        // Get the entry with seq number 4
        let entry_at_seq_num_four = entries.get(3).unwrap();

        // Reconstruct it with an invalid backlink
        let entry_at_seq_num_four_with_wrong_backlink = Entry::new(
            &entry_at_seq_num_four.log_id(),
            Some(&Operation::from(&operation_encoded)),
            entry_at_seq_num_four.skiplink_hash().as_ref(),
            Some(&Hash::new_from_bytes(vec![1, 2, 3]).unwrap()),
            &entry_at_seq_num_four.seq_num(),
        )
        .unwrap();

        let entry_at_seq_num_four_with_wrong_backlink =
            sign_and_encode(&entry_at_seq_num_four_with_wrong_backlink, &key_pair).unwrap();

        let entry_at_seq_num_four_with_wrong_backlink = StorageEntry::new(
            &entry_at_seq_num_four_with_wrong_backlink,
            &operation_encoded,
        )
        .unwrap();

        assert_eq!(
            test_db
                .try_get_backlink(&entry_at_seq_num_four_with_wrong_backlink)
                .await
                .unwrap_err()
                .to_string(),
            format!(
                "The backlink hash encoded in the entry: {} did not match the expected backlink hash",
                entry_at_seq_num_four_with_wrong_backlink.hash()
            )
        );
    }

    #[rstest(
        case(1, None),
        case(2, None),
        case(3, None),
        case(4, Some(1)),
        case(5, None),
        case(6, None),
        case(7, None),
        case(8, Some(4)),
        case(9, None),
        case(10, None),
        case(11, None),
        case(12, Some(8)),
        case(13, Some(4)),
        case(14, None),
        case(15, None),
        case(16, None)
    )]
    #[async_std::test]
    async fn try_get_skiplink(
        #[case] seq_num: usize,
        #[case] expected_skiplink_seq_num: Option<usize>,
        test_db: SimplestStorageProvider,
    ) {
        let entries = test_db.entries.lock().unwrap().clone();

        let expected_skiplink =
            expected_skiplink_seq_num.map(|seq_num| entries.get(seq_num - 1).cloned().unwrap());

        assert_eq!(
            expected_skiplink,
            test_db
                .try_get_skiplink(&entries[seq_num - 1])
                .await
                .unwrap()
        );
    }

    #[rstest]
    #[async_std::test]
    async fn try_get_skiplink_entry_missing(test_db: SimplestStorageProvider) {
        let entries = test_db.entries.lock().unwrap().clone();

        // Get the entry with seq number 4
        let entry_at_seq_num_four = entries.get(3).unwrap();

        {
            // Remove the entry at seq_num 1, which is the expected skiplink for the above seq num 4 entry
            test_db.entries.lock().unwrap().remove(0);
        }
        assert_eq!(
            test_db
                .try_get_skiplink(entry_at_seq_num_four)
                .await
                .unwrap_err()
                .to_string(),
            format!(
                "Could not find expected skiplink in database for entry with id: {}",
                entry_at_seq_num_four.hash()
            )
        );
    }

    #[rstest]
    #[async_std::test]
    async fn try_get_skiplink_invalid_skiplink(
        key_pair: KeyPair,
        operation_encoded: OperationEncoded,
        test_db: SimplestStorageProvider,
    ) {
        let entries = test_db.entries.lock().unwrap().clone();

        // Get the entry with seq number 4
        let entry_at_seq_num_four = entries.get(3).unwrap();

        // Reconstruct it with an invalid skiplink
        let entry_at_seq_num_four_with_wrong_skiplink = Entry::new(
            &entry_at_seq_num_four.log_id(),
            Some(&Operation::from(&operation_encoded)),
            Some(&Hash::new_from_bytes(vec![1, 2, 3]).unwrap()),
            entry_at_seq_num_four.backlink_hash().as_ref(),
            &entry_at_seq_num_four.seq_num(),
        )
        .unwrap();

        let entry_at_seq_num_four_with_wrong_skiplink =
            sign_and_encode(&entry_at_seq_num_four_with_wrong_skiplink, &key_pair).unwrap();

        let entry_at_seq_num_four_with_wrong_skiplink = StorageEntry::new(
            &entry_at_seq_num_four_with_wrong_skiplink,
            &operation_encoded,
        )
        .unwrap();

        assert_eq!(
            test_db
                .try_get_skiplink(&entry_at_seq_num_four_with_wrong_skiplink)
                .await
                .unwrap_err()
                .to_string(),
            format!(
                "The skiplink hash encoded in the entry: {} did not match the known hash of the skiplink target",
                entry_at_seq_num_four_with_wrong_skiplink.hash()
            )
        );
    }

    #[rstest]
    #[async_std::test]
    async fn can_determine_next_skiplink(test_db: SimplestStorageProvider) {
        let entries = test_db.entries.lock().unwrap().clone();
        for seq_num in 1..10 {
            let current_entry = entries.get(seq_num - 1).unwrap();
            let next_entry_skiplink = test_db.determine_next_skiplink(current_entry).await;
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

        let error_response = new_db
            .determine_next_skiplink(entries.get(6).unwrap())
            .await;

        assert_eq!(
            format!("{}", error_response.unwrap_err()),
            "Could not find expected skiplink entry in database"
        )
    }

    #[rstest]
    #[async_std::test]
    async fn get_n_entries(key_pair: KeyPair, test_db: SimplestStorageProvider) {
        let author = Author::try_from(*key_pair.public_key()).unwrap();
        let log_id = LogId::default();

        let five_entries = test_db
            .get_paginated_log_entries(&author, &log_id, &SeqNum::new(1).unwrap(), 5)
            .await
            .unwrap();
        assert_eq!(five_entries.len(), 5);

        let end_of_log_reached = test_db
            .get_paginated_log_entries(&author, &log_id, &SeqNum::new(1).unwrap(), 1000)
            .await
            .unwrap();
        assert_eq!(end_of_log_reached.len(), 16);

        let first_entry_not_found = test_db
            .get_paginated_log_entries(&author, &log_id, &SeqNum::new(10000).unwrap(), 1)
            .await
            .unwrap();
        assert!(first_entry_not_found.is_empty());
    }

    #[rstest]
    #[async_std::test]
    async fn get_cert_pool(key_pair: KeyPair, test_db: SimplestStorageProvider) {
        let author = Author::try_from(*key_pair.public_key()).unwrap();
        let log_id = LogId::default();

        let cert_pool = test_db
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
