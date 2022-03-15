use async_trait::async_trait;
use bamboo_rs_core_ed25519_yasmf::entry::is_lipmaa_required;

use crate::entry::LogId;
use crate::entry::SeqNum;
use crate::hash::Hash;
use crate::identity::Author;
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
    async fn by_schema(&self, schema: &Hash) -> Result<Vec<StorageEntry>, EntryStorageError>;

    /// Determine skiplink entry hash ("lipmaa"-link) for entry in this log, return `None` when no
    /// skiplink is required for the next entry.
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
