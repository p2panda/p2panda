// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;
use std::convert::TryFrom;
use std::fmt::Debug;

use crate::entry::SeqNum;
use crate::hash::Hash;
use crate::{entry::LogId, identity::Author};

/// Params passed when a log is requested
#[derive(Debug)]
pub struct GetLogParams {
    pub author: Author,
    pub document_id: Hash,
}

/// Trait implemented on all types which are to be stored into memory store.
pub trait ToMemoryStore {
    /// The returned type
    type Output;
    /// The error type
    type ToMemoryStoreError: Debug;
    /// Returns a data store friendly conversion of this type.
    fn to_store_value(self) -> Result<Self::Output, Self::ToMemoryStoreError>;
}

/// Trait implemented on all implementation specific types which are retrieved from memory store.
pub trait FromMemoryStore {
    /// The returned type
    type Output;
    /// The error type
    type FromMemoryStoreError: Debug;
    /// Returns a returns the in memory (probably a p2panda_rs type) conversion of this type.
    fn from_store_value(self) -> Result<Self::Output, Self::FromMemoryStoreError>;
}

/// Trait used for inserting items into the memory store.
///
/// Should be implemented on every type an implementation handles.
#[async_trait]
pub trait Insert<Type: ToMemoryStore>: Send + Sync {
    /// The error type
    type InsertError: Debug;
    /// Insert item into the memory store.
    async fn insert(&self, value: Type) -> Result<bool, Self::InsertError>;
}

/// Trait used for getting items from the memory store.
///
/// Should be implemented on every type an implementation handles.
#[async_trait]
pub trait Get: Send + Sync {
    /// The error type
    type GetError: Debug;
    /// The returned type
    type Output: FromMemoryStore;

    /// Get an item from the memory store.
    async fn get<Type>(
        &self,
        get_params: GetLogParams,
    ) -> Result<Option<Self::Output>, Self::GetError>;
}

/// Trait which handles all memory store actions relating to `Log`s.
#[async_trait]
pub trait LogStore {
    /// The error type
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

/// Trait which handles all memory store actions relating to `Entries`.
#[async_trait]
pub trait EntryStore {
    /// Type representing an entry as it is stored in the memory store
    type StoredEntry: FromMemoryStore + TryFrom<Self::EntryRow>;
    /// An internal type representing an enty row (here because of an `aquadoggo` quirk)
    type EntryRow;
    /// The error type
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

/// All other methods needed to be implemented by a p2panda `MemoryStore`
#[async_trait]
pub trait MemoryStore: LogStore + EntryStore {
    /// The error type
    type Error: Debug;

    /// Returns the related document for any entry.
    ///
    /// Every entry is part of a document and, through that, associated with a specific log id used
    /// by this document and author. This method returns that document id by looking up the log
    /// that the entry was stored in.
    async fn get_document_by_entry(&self, entry_hash: &Hash) -> Result<Option<Hash>, Self::Error>;
}
