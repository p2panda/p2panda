// SPDX-License-Identifier: AGPL-3.0-or-later

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
    /// Insert an entry into storage.
    ///
    /// Returns an error if a fatal storage error occured.
    async fn insert_entry(&self, value: StorageEntry) -> Result<(), EntryStorageError>;

    /// Get an entry at sequence position within an author's log.
    ///
    /// Returns a result containing an entry wrapped in an option. If no entry could
    /// be found at this author - log - seq number location then None is returned.
    /// Errors when a fatal storage error occurs.
    async fn get_entry_at_seq_num(
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
            .get_entry_at_seq_num(&entry.author(), &entry.log_id(), &backlink_seq_num)
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
                    .get_entry_at_seq_num(&entry.author(), &entry.log_id(), &seq_num)
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
    async fn get_latest_entry(
        &self,
        author: &Author,
        log_id: &LogId,
    ) -> Result<Option<StorageEntry>, EntryStorageError>;

    /// Get a vector of all entries of a given schema.
    ///
    /// Returns a result containing vector of entries wrapped in an option.
    /// If no schema with this id could be found then None is returned.
    /// Errors when a fatal storage error occurs.
    async fn get_entries_by_schema(
        &self,
        schema: &SchemaId,
    ) -> Result<Vec<StorageEntry>, EntryStorageError>;

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
                .get_entry_at_seq_num(&entry.author(), &entry.log_id(), &skiplink_seq_num)
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
    use std::sync::{Arc, Mutex};

    use rstest::rstest;

    use crate::entry::{sign_and_encode, Entry};
    use crate::hash::Hash;
    use crate::identity::KeyPair;
    use crate::operation::{Operation, OperationEncoded};
    use crate::storage_provider::traits::test_utils::{test_db, TestStore};
    use crate::storage_provider::traits::{AsStorageEntry, EntryStore};
    use crate::test_utils::constants::SKIPLINK_ENTRIES;
    use crate::test_utils::db::{SimplestStorageProvider, StorageEntry};
    use crate::test_utils::fixtures::{key_pair, operation_encoded};

    #[rstest]
    #[async_std::test]
    async fn try_get_backlink(
        #[values[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14 ,15 ,16]] seq_num: usize,
        #[from(test_db)]
        #[with(17, 1, 1)]
        #[future]
        db: TestStore,
    ) {
        let db = db.await;
        let entries = db.store.entries.lock().unwrap().clone();

        let entry = entries
            .values()
            .find(|entry| entry.seq_num().as_u64() as usize == seq_num)
            .unwrap();

        let backlink = if seq_num == 1 {
            None
        } else {
            entries
                .values()
                .find(|entry| entry.seq_num().as_u64() as usize == seq_num - 1)
        };

        assert_eq!(
            backlink,
            db.store.try_get_backlink(entry).await.unwrap().as_ref()
        );
    }

    #[rstest]
    #[async_std::test]
    async fn try_get_backlink_entry_missing(
        #[from(test_db)]
        #[with(17, 1, 1)]
        #[future]
        db: TestStore,
    ) {
        let db = db.await;
        let entries = db.store.entries.lock().unwrap().clone();

        // Get the entry with seq number 1
        let entry_at_seq_num_one = entries
            .values()
            .find(|entry| entry.seq_num().as_u64() as usize == 1)
            .unwrap();

        // Get the entry with seq number 2
        let entry_at_seq_num_two = entries
            .values()
            .find(|entry| entry.seq_num().as_u64() as usize == 2)
            .unwrap();

        {
            // Remove the entry at seq_num 1, which is the expected backlink for the above seq num 2 entry
            db.store
                .entries
                .lock()
                .unwrap()
                .remove(&entry_at_seq_num_one.hash());
        }

        assert_eq!(
            db.store
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
        #[from(test_db)]
        #[with(4, 1, 1)]
        #[future]
        db: TestStore,
    ) {
        let db = db.await;
        let entries = db.store.entries.lock().unwrap().clone();

        // Get the entry with seq number 4
        let entry_at_seq_num_four = entries
            .values()
            .find(|entry| entry.seq_num().as_u64() as usize == 4)
            .unwrap();

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
            db.store
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
        #[from(test_db)]
        #[with(17, 1, 1)]
        #[future]
        db: TestStore,
    ) {
        let db = db.await;
        let entries = db.store.entries.lock().unwrap().clone();

        let entry = entries
            .values()
            .find(|entry| entry.seq_num().as_u64() as usize == seq_num)
            .unwrap();

        let expected_skiplink = expected_skiplink_seq_num.map(|seq_num| {
            entries
                .values()
                .find(|entry| entry.seq_num().as_u64() as usize == seq_num)
                .unwrap()
        });

        // Get the entry with seq number 4
        assert_eq!(
            expected_skiplink,
            db.store.try_get_skiplink(entry).await.unwrap().as_ref()
        );
    }

    #[rstest]
    #[async_std::test]
    async fn try_get_skiplink_entry_missing(
        #[from(test_db)]
        #[with(4, 1, 1)]
        #[future]
        db: TestStore,
    ) {
        let db = db.await;
        let entries = db.store.entries.lock().unwrap().clone();

        // Get the entry with seq number 1
        let entry_at_seq_num_one = entries
            .values()
            .find(|entry| entry.seq_num().as_u64() as usize == 1)
            .unwrap();

        // Get the entry with seq number 4
        let entry_at_seq_num_four = entries
            .values()
            .find(|entry| entry.seq_num().as_u64() as usize == 4)
            .unwrap();

        {
            // Remove the entry at seq_num 1, which is the expected skiplink for the above seq num 4 entry
            db.store
                .entries
                .lock()
                .unwrap()
                .remove(&entry_at_seq_num_one.hash());
        }

        assert_eq!(
            db.store
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
        #[from(test_db)]
        #[with(4, 1, 1)]
        #[future]
        db: TestStore,
    ) {
        let db = db.await;
        let entries = db.store.entries.lock().unwrap().clone();

        // Get the entry with seq number 4
        let entry_at_seq_num_four = entries
            .values()
            .find(|entry| entry.seq_num().as_u64() as usize == 4)
            .unwrap();

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
            db.store
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
    async fn can_determine_next_skiplink(
        #[from(test_db)]
        #[with(17, 1, 1)]
        #[future]
        db: TestStore,
    ) {
        let db = db.await;
        let entries = db.store.entries.lock().unwrap().clone();

        for seq_num in 1..10 {
            let current_entry = entries
                .values()
                .find(|entry| entry.seq_num().as_u64() as usize == seq_num)
                .unwrap();
            let next_entry_skiplink = db.store.determine_next_skiplink(current_entry).await;
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
    async fn skiplink_does_not_exist(
        #[from(test_db)]
        #[with(17, 1, 1)]
        #[future]
        db: TestStore,
    ) {
        let db = db.await;
        let entries = db.store.entries.lock().unwrap().clone();

        let mut log_entries_with_skiplink_missing = entries.clone();

        // Get the entry with seq number 4
        let entry_at_seq_num_four = entries
            .values()
            .find(|entry| entry.seq_num().as_u64() as usize == 4)
            .unwrap();

        // Remove the entry at seq num 4
        log_entries_with_skiplink_missing.remove(&entry_at_seq_num_four.hash());

        // Get the entry with seq number 7
        let entry_at_seq_num_seven = entries
            .values()
            .find(|entry| entry.seq_num().as_u64() as usize == 7)
            .unwrap();

        // Construct a new db which is missing one entry
        let new_db = SimplestStorageProvider {
            entries: Arc::new(Mutex::new(log_entries_with_skiplink_missing)),
            ..SimplestStorageProvider::default()
        };

        let error_response = new_db.determine_next_skiplink(entry_at_seq_num_seven).await;

        assert_eq!(
            format!("{}", error_response.unwrap_err()),
            "Could not find expected skiplink entry in database"
        )
    }
}
