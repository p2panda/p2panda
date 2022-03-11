// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;
use std::fmt::Debug;

use crate::entry::{Entry, SeqNum};
use crate::hash::Hash;
use crate::{entry::LogId, identity::Author};

pub struct PublishEntryResponse {
    pub entry_hash_backlink: Option<Hash>,
    pub entry_hash_skiplink: Option<Hash>,
    pub seq_num: String,
    pub log_id: String,
}

pub trait FromMemoryStoreValue: Sized {
    type Error: Debug;
    fn from_store_value(ksv: &[u8]) -> Result<Self, Self::Error>;
}

pub trait ToMemoryStoreValue {
    type Error: Debug;
    fn to_store_value(&self) -> Result<Vec<u8>, Self::Error>;
}

#[async_trait]
pub trait Insert<Modal>: Send + Sync {
    type Error: Debug;

    async fn insert(&self, log: Modal) -> Result<bool, Self::Error>;
}

#[async_trait]
pub trait MemoryStore {
    type Error: Debug;

    /// Determines the next unused log_id of an author.
    async fn next_log_id(&self, author: &Author) -> Result<LogId, Self::Error>;

    /// Returns registered or possible log id for a document.
    ///
    /// If no log has been previously registered for this document it automatically returns the
    /// next unused log_id.
    async fn find_document_log_id(
        &self,
        author: &Author,
        document_id: Option<&Hash>,
    ) -> Result<LogId, Self::Error>;

    /// Returns the related document for any entry.
    ///
    /// Every entry is part of a document and, through that, associated with a specific log id used
    /// by this document and author. This method returns that document id by looking up the log
    /// that the entry was stored in.
    async fn get_document_by_entry(&self, entry_hash: &Hash) -> Result<Option<Hash>, Self::Error>;

    /// Returns entry at sequence position within an author's log.
    async fn entry_at_seq_num(
        &self,
        author: &Author,
        log_id: &LogId,
        seq_num: &SeqNum,
    ) -> Result<Option<Entry>, Self::Error>;

    /// Returns the latest Bamboo entry of an author's log.
    async fn latest_entry(
        &self,
        author: &Author,
        log_id: &LogId,
    ) -> Result<Option<Entry>, Self::Error>;

    /// Determine skiplink entry hash ("lipmaa"-link) for entry in this log, return `None` when no
    /// skiplink is required for the next entry.
    async fn determine_skiplink(&self, entry: &Entry) -> Result<Option<Hash>, Self::Error>;
}
