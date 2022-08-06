// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;

use crate::next::entry::LogId;
use crate::next::entry::SeqNum;
use crate::next::hash::Hash;
use crate::next::identity::Author;
use crate::next::schema::SchemaId;
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
