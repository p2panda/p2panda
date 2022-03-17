use async_trait::async_trait;
use bamboo_rs_core_ed25519_yasmf::entry::is_lipmaa_required;

use crate::entry::LogId;
use crate::entry::SeqNum;
use crate::hash::Hash;
use crate::identity::Author;
use crate::schema::SchemaId;
use crate::storage_provider::errors::EntryStorageError;
use crate::storage_provider::traits::AsStorageEntry;

/// Trait which handles all storage actions relating to `Entries`s.
///
/// This trait should be implemented on the root storage provider struct. It's definitions
/// make up the required methods for inserting and querying entries from storage.
#[async_trait]
pub trait EntryStore<StorageEntry: AsStorageEntry> {
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

    /// Return vector of all entries of a given schema
    async fn by_schema(&self, schema: &SchemaId) -> Result<Vec<StorageEntry>, EntryStorageError>;

    /// Determine skiplink entry hash ("lipmaa"-link) for entry in this log, return `None` when no
    /// skiplink is required for the next entry.
    async fn determine_skiplink(
        &self,
        storage_entry: &StorageEntry,
    ) -> Result<Option<Hash>, EntryStorageError> {
        let next_seq_num = storage_entry
            .entry_decoded()
            .seq_num()
            .clone()
            .next()
            .unwrap();

        // Unwrap as we know that an skiplink exists as soon as previous entry is given
        let skiplink_seq_num = next_seq_num.skiplink_seq_num().unwrap();

        // Check if skiplink is required and return hash if so
        let entry_skiplink_hash = if is_lipmaa_required(next_seq_num.as_u64()) {
            let skiplink_entry = self
                .entry_at_seq_num(
                    &storage_entry.entry_encoded().author(),
                    storage_entry.entry_decoded().log_id(),
                    &skiplink_seq_num,
                )
                .await?
                .unwrap();
            Some(skiplink_entry.entry_encoded().hash())
        } else {
            None
        };

        Ok(entry_skiplink_hash)
    }
}

#[cfg(test)]
pub mod tests {

    use async_trait::async_trait;
    use bamboo_rs_core_ed25519_yasmf::entry::is_lipmaa_required;
    use rstest::rstest;
    use std::convert::TryFrom;
    use std::sync::{Arc, Mutex};

    use crate::document::DocumentId;
    use crate::entry::{sign_and_encode, Entry, EntrySigned, LogId, SeqNum};
    use crate::identity::{Author, KeyPair};
    use crate::operation::{AsOperation, Operation, OperationEncoded};
    use crate::schema::SchemaId;
    use crate::storage_provider::errors::EntryStorageError;
    use crate::storage_provider::models::Log;
    use crate::storage_provider::traits::test_setup::{SimplestStorageProvider, StorageEntry};
    use crate::storage_provider::traits::{AsStorageEntry, EntryStore};
    use crate::test_utils::fixtures::{
        create_operation, document_id, entry, entry_signed_encoded, operation_encoded,
        random_key_pair, schema, update_operation,
    };

    /// Implement `EntryStore` trait on `SimplestStorageProvider`
    #[async_trait]
    impl EntryStore<StorageEntry> for SimplestStorageProvider {
        /// Insert an entry into storage.
        async fn insert_entry(&self, entry: StorageEntry) -> Result<bool, EntryStorageError> {
            let mut entries = self.entries.lock().unwrap();
            entries.push(entry);
            // Remove duplicate entries.
            entries.dedup();
            Ok(true)
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
                entry.entry_encoded().author() == *author
                    && entry.entry_decoded().log_id() == log_id
                    && entry.entry_decoded().seq_num() == seq_num
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
                .filter(|entry| {
                    entry.entry_encoded().author() == *author
                        && entry.entry_decoded().log_id() == log_id
                })
                .max_by_key(|entry| entry.entry_decoded().seq_num().as_u64());

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
                .filter(|entry| entry.entry_decoded().operation().unwrap().schema() == *schema)
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

        let storage_entry = StorageEntry::new(entry_signed_encoded, operation_encoded);
        let decoded_entry = storage_entry.entry_decoded();

        // Insert an entry into the store.
        assert!(store.insert_entry(storage_entry.clone()).await.is_ok());

        let author = storage_entry.entry_encoded().author();

        // Get an entry at a specific seq number from an authors log.
        let entry_at_seq_num = store
            .entry_at_seq_num(&author, decoded_entry.log_id(), decoded_entry.seq_num())
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

        let storage_entry = StorageEntry::new(entry_signed_encoded, operation_encoded);

        let author = storage_entry.entry_encoded().author();

        // Before an entry is inserted the latest entry should be none.
        assert!(store
            .latest_entry(&author, &LogId::default())
            .await
            .unwrap()
            .is_none());

        // Insert an entry into the store.
        assert!(store.insert_entry(storage_entry.clone()).await.is_ok());

        assert_eq!(
            store
                .latest_entry(&author, &LogId::default())
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
        let author_1_entry = StorageEntry::new(author_1_entry, operation_encoded.clone());
        let author_2_entry = StorageEntry::new(author_2_entry, operation_encoded);

        // Before an entry with this schema is inserted this method should return an empty array.
        assert!(store.by_schema(&schema).await.unwrap().is_empty());

        // Insert two entries into the store.
        store.insert_entry(author_1_entry).await.unwrap();
        store.insert_entry(author_2_entry).await.unwrap();

        assert_eq!(store.by_schema(&schema).await.unwrap().len(), 2);
    }

    #[rstest]
    #[async_std::test]
    async fn can_determine_skiplink(
        #[from(random_key_pair)] key_pair: KeyPair,
        create_operation: Operation,
        update_operation: Operation,
        schema: SchemaId,
        document_id: DocumentId,
    ) {
        let skiplink_entries = [3, 7];
        let author = Author::try_from(key_pair.public_key().to_owned()).unwrap();
        let mut db_entries: Vec<StorageEntry> = vec![];
        let db_logs = vec![Log::new(&author, &schema, &document_id, &LogId::new(1)).into()];

        let create_entry = entry(
            create_operation.clone(),
            SeqNum::new(1).unwrap(),
            None,
            None,
        );

        let encoded_entry = sign_and_encode(&create_entry, &key_pair).unwrap();
        let encoded_operation = OperationEncoded::try_from(&create_operation).unwrap();
        let storage_entry = StorageEntry::new(encoded_entry, encoded_operation);
        db_entries.push(storage_entry);

        for seq_num in 2..10 {
            let seq_num = SeqNum::new(seq_num).unwrap();
            let mut skiplink = None;
            let backlink = db_entries
                .get(seq_num.as_u64() as usize - 2)
                .unwrap()
                .entry_encoded()
                .hash();

            if skiplink_entries.contains(&seq_num.as_u64()) {
                let skiplink_seq_num = seq_num.skiplink_seq_num().unwrap();
                skiplink = Some(
                    db_entries
                        .get(skiplink_seq_num.as_u64() as usize - 1)
                        .unwrap()
                        .entry_encoded()
                        .hash(),
                );
            };

            let update_entry = entry(update_operation.clone(), seq_num, Some(backlink), skiplink);
            let encoded_entry = sign_and_encode(&update_entry, &key_pair).unwrap();
            let encoded_operation = OperationEncoded::try_from(&update_operation).unwrap();
            let storage_entry = StorageEntry::new(encoded_entry, encoded_operation);

            db_entries.push(storage_entry)
        }

        // Instantiate a SimpleStorage with already existing entry and log values stored.
        let store = SimplestStorageProvider {
            logs: Arc::new(Mutex::new(db_logs)),
            entries: Arc::new(Mutex::new(db_entries.clone())),
        };

        for entry in db_entries {
            let result = store.determine_skiplink(&entry).await;
            assert!(result.is_ok());
            if skiplink_entries.contains(&entry.entry_decoded().seq_num().as_u64()) {
                assert!(result.unwrap().is_some());
            } else {
                assert!(result.unwrap().is_none())
            }
        }
    }
}
