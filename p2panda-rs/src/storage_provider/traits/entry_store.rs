// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;

use crate::entry::traits::{AsEncodedEntry, AsEntry};
use crate::entry::SeqNum;
use crate::entry::{EncodedEntry, Entry as P2pandaEntry, LogId};
use crate::hash::Hash;
use crate::identity::PublicKey;
use crate::operation::EncodedOperation;
use crate::schema::SchemaId;
use crate::storage_provider::error::EntryStorageError;

/// Trait which handles all storage actions relating to entries.
///
/// This trait should be implemented on the root storage provider struct. It's definitions make up
/// the required methods for inserting and querying entries from storage.
#[async_trait]
pub trait EntryStore {
    /// An associated type representing an entry retrieved from storage.
    type Entry: AsEntry + AsEncodedEntry + Into<P2pandaEntry>;

    /// Insert an entry into storage.
    ///
    /// No validation of the passed values occurs, this is assumed to have already happened
    /// elsewhere.
    ///
    /// Returns an error if a fatal storage error occured.
    async fn insert_entry(
        &self,
        entry: &P2pandaEntry,
        encoded_entry: &EncodedEntry,
        encoded_operation: Option<&EncodedOperation>,
    ) -> Result<(), EntryStorageError>;

    /// Get an entry at sequence position within a public key's log.
    ///
    /// Returns a result containing an entry wrapped in an option. If no entry could
    /// be found at this public key - log - seq number location then None is returned.
    /// Errors when a fatal storage error occurs.
    async fn get_entry_at_seq_num(
        &self,
        public_key: &PublicKey,
        log_id: &LogId,
        seq_num: &SeqNum,
    ) -> Result<Option<Self::Entry>, EntryStorageError>;

    /// Get an entry by it's hash.
    async fn get_entry_by_hash(&self, hash: &Hash) -> Result<Option<Self::Entry>, EntryStorageError>;

    /// Get the latest Bamboo entry of public key's log.
    ///
    /// Returns a result containing an entry wrapped in an option. If no log was
    /// could be found at this public key - log location then None is returned.
    /// Errors when a fatal storage error occurs.
    async fn get_latest_entry(
        &self,
        public_key: &PublicKey,
        log_id: &LogId,
    ) -> Result<Option<Self::Entry>, EntryStorageError>;

    /// Get a vector of all entries of a given schema.
    ///
    /// Returns a result containing vector of entries wrapped in an option.
    /// If no schema with this id could be found then None is returned.
    /// Errors when a fatal storage error occurs.
    async fn get_entries_by_schema(
        &self,
        schema: &SchemaId,
    ) -> Result<Vec<Self::Entry>, EntryStorageError>;

    /// Get all entries of a log from a specified sequence number up to passed max number of entries.
    ///
    /// Returns a vector of entries the length of which will not be greater than the max number
    /// passed into the method. Fewer may be returned if the end of the log is reached.
    async fn get_paginated_log_entries(
        &self,
        public_key: &PublicKey,
        log_id: &LogId,
        seq_num: &SeqNum,
        max_number_of_entries: usize,
    ) -> Result<Vec<Self::Entry>, EntryStorageError>;

    /// Get all entries which make up the certificate pool for the given entry.
    ///
    /// Returns a result containing vector of entries wrapped in an option. If no entry
    /// could be found at this public key - log - seq number location then an error is
    /// returned.
    async fn get_certificate_pool(
        &self,
        author_id: &PublicKey,
        log_id: &LogId,
        seq_num: &SeqNum,
    ) -> Result<Vec<Self::Entry>, EntryStorageError>;
}
