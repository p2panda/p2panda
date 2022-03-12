// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;
use std::convert::TryFrom;
use std::fmt::Debug;

use crate::entry::SeqNum;
use crate::hash::Hash;
use crate::{entry::LogId, identity::Author};

pub struct GetLogParams {
    pub author: Author,
    pub document_id: Hash,
}

pub trait ToMemoryStore {
    type Output;
    type ToMemoryStoreError: Debug;
    fn to_store_value(self) -> Result<Self::Output, Self::ToMemoryStoreError>;
}

pub trait FromMemoryStore {
    type Output;
    type FromMemoryStoreError: Debug;
    fn from_store_value(self) -> Result<Self::Output, Self::FromMemoryStoreError>;
}

#[async_trait]
pub trait Insert<Type: ToMemoryStore>: Send + Sync {
    type InsertError: Debug;

    async fn insert(&self, value: Type) -> Result<bool, Self::InsertError>;
}

#[async_trait]
pub trait Get: Send + Sync {
    type GetError: Debug;
    type Output: FromMemoryStore;

    async fn get<Type>(
        &self,
        get_params: GetLogParams,
    ) -> Result<Option<Self::Output>, Self::GetError>;
}

#[async_trait]
pub trait LogStore {
    type LogError: Debug;

    /// Returns registered or possible log id for a document.
    ///
    /// If no log has been previously registered for this document it automatically returns the
    /// next unused log_id.
    async fn find_document_log_id(
        &self,
        author: &Author,
        document_id: Option<&Hash>,
    ) -> Result<LogId, Self::LogError>;
    /// Determines the next unused log_id of an author.
    async fn next_log_id(&self, author: &Author) -> Result<LogId, Self::LogError>;
}

#[async_trait]
pub trait EntryStore {
    type StoredEntry: FromMemoryStore + TryFrom<Self::EntryRow>;
    type EntryRow;
    type EntryError: Debug;

    /// Returns entry at sequence position within an author's log.
    async fn entry_at_seq_num(
        &self,
        author: &Author,
        log_id: &LogId,
        seq_num: &SeqNum,
    ) -> Result<Option<Self::StoredEntry>, Self::EntryError>;

    /// Returns the latest Bamboo entry of an author's log.
    async fn latest_entry(
        &self,
        author: &Author,
        log_id: &LogId,
    ) -> Result<Option<Self::StoredEntry>, Self::EntryError>;

    /// Return vector of all entries of a given schema
    // @TODO: This currently returns `EntryRow`, a better API would return `Entry` instead as it is
    // properly typed and `EntryRow` is only meant as an intermediate struct to deal with
    // databases. Here we still return `EntryRow` for the `queryEntries` RPC response (we want
    // `seq_num` and `log_id` to be strings). This should be changed as soon as we move over using
    // a GraphQL API.
    async fn by_schema(&self, schema: &Hash) -> Result<Vec<Self::EntryRow>, Self::EntryError>;

    /// Determine skiplink entry hash ("lipmaa"-link) for entry in this log, return `None` when no
    /// skiplink is required for the next entry.
    async fn determine_skiplink(
        &self,
        entry: &Self::StoredEntry,
    ) -> Result<Option<Hash>, Self::EntryError>;
}

#[async_trait]
pub trait MemoryStore: LogStore + EntryStore {
    type Error: Debug;

    /// Returns the related document for any entry.
    ///
    /// Every entry is part of a document and, through that, associated with a specific log id used
    /// by this document and author. This method returns that document id by looking up the log
    /// that the entry was stored in.
    async fn get_document_by_entry(&self, entry_hash: &Hash) -> Result<Option<Hash>, Self::Error>;
}
