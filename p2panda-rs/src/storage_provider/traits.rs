// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;
use bamboo_rs_core_ed25519_yasmf::entry::is_lipmaa_required;
use std::fmt::Debug;

use crate::entry::SeqNum;
use crate::hash::Hash;
use crate::Validate;
use crate::{entry::LogId, identity::Author};

use super::models::{AsEntry, AsLog};
use super::requests::AsEntryArgsRequest;
use super::responses::AsEntryArgsResponse;
use super::StorageProviderError;

/// Trait which handles all storage actions relating to `Log`s.
#[async_trait]
pub trait LogStore<T> {
    /// The error type
    type LogError: Debug;
    /// The type representing a Log
    ///
    /// NB: Interestingly, there is no struct representing this in p2panda_rs,
    /// but that is all cool, thank you generics ;-p
    type Log: AsLog<T> + Send;

    /// Insert a log into storage.
    async fn insert_log(&self, value: Self::Log) -> Result<bool, Self::LogError>;

    /// Get a log from storage
    async fn get(
        &self,
        author: &Author,
        document_id: &Hash,
    ) -> Result<Option<LogId>, Self::LogError>;

    /// Returns registered or possible log id for a document.
    ///
    /// If no log has been previously registered for this document it
    /// automatically returns the next unused log_id.

    /// Returns registered or possible log id for a document.
    ///
    /// If no log has been previously registered for this document it
    /// automatically returns the next unused log_id.
    async fn find_document_log_id(
        &self,
        author: &Author,
        document_id: Option<&Hash>,
    ) -> Result<LogId, Self::LogError> {
        // Determine log_id for this document when a hash was given
        let document_log_id = match document_id {
            Some(id) => self.get(author, id).await?,
            None => None,
        };

        // Use result or find next possible log_id automatically when nothing was found yet
        let log_id = match document_log_id {
            Some(value) => value,
            None => self.next_log_id(author).await?,
        };

        Ok(log_id)
    }
    /// Determines the next unused log_id of an author.
    async fn next_log_id(&self, author: &Author) -> Result<LogId, Self::LogError>;
}

/// Trait which handles all storage actions relating to `Entries`.
#[async_trait]
pub trait EntryStore<T> {
    /// Type representing an entry, must implement the `AsEntry` trait.
    type Entry: AsEntry<T>;
    /// The error type
    type EntryError: Debug;

    /// Insert an entry into storage.
    async fn insert_entry(&self, value: Self::Entry) -> Result<bool, Self::EntryError>;

    /// Returns entry at sequence position within an author's log.
    async fn entry_at_seq_num(
        &self,
        author: &Author,
        log_id: &LogId,
        seq_num: &SeqNum,
    ) -> Result<Option<Self::Entry>, Self::EntryError>;

    /// Returns the latest Bamboo entry of an author's log.
    async fn latest_entry(
        &self,
        author: &Author,
        log_id: &LogId,
    ) -> Result<Option<Self::Entry>, Self::EntryError>;

    /// Return vector of all entries of a given schema
    async fn by_schema(&self, schema: &Hash) -> Result<Vec<Self::Entry>, Self::EntryError>;

    /// Determine skiplink entry hash ("lipmaa"-link) for entry in this log, return `None` when no
    /// skiplink is required for the next entry.
    /// Determine skiplink entry hash ("lipmaa"-link) for entry in this log, return `None` when no
    /// skiplink is required for the next entry.
    async fn determine_skiplink(
        &self,
        entry: &Self::Entry,
    ) -> Result<Option<Hash>, Self::EntryError> {
        let next_seq_num = entry.seq_num().clone().next().unwrap();

        // Unwrap as we know that an skiplink exists as soon as previous entry is given
        let skiplink_seq_num = next_seq_num.skiplink_seq_num().unwrap();

        // Check if skiplink is required and return hash if so
        let entry_skiplink_hash = if is_lipmaa_required(next_seq_num.as_u64()) {
            let skiplink_entry = self
                .entry_at_seq_num(&entry.author(), &entry.log_id(), &skiplink_seq_num)
                .await?
                .unwrap();
            Some(skiplink_entry.entry_hash())
        } else {
            None
        };

        Ok(entry_skiplink_hash)
    }
}

/// All other methods needed to be implemented by a p2panda `StorageProvider`
#[async_trait]
pub trait StorageProvider<T, U>: EntryStore<T> + LogStore<U> {
    /// The error type
    type Error: Debug + Send + Sync;
    type EntryArgsResponse: AsEntryArgsResponse + Send + Sync;
    /// Returns the related document for any entry.
    ///
    /// Every entry is part of a document and, through that, associated with a specific log id used
    /// by this document and author. This method returns that document id by looking up the log
    /// that the entry was stored in.
    async fn get_document_by_entry(&self, entry_hash: &Hash) -> Result<Option<Hash>, Self::Error>;

    /// Implementation of `panda_getEntryArguments` RPC method.
    ///
    /// Returns required data (backlink and skiplink entry hashes, last sequence number and the
    /// document's log_id) to encode a new bamboo entry.
    async fn get_entry_args(
        &self,
        author: &Author,
        document: Option<&Hash>,
    ) -> Result<Self::EntryArgsResponse, StorageProviderError> {
        // Validate `author` request parameter
        author.validate().map_err(|_| StorageProviderError::Error)?;

        // Validate `document` request parameter when it is set
        let document = match document {
            Some(doc) => {
                doc.validate().map_err(|_| StorageProviderError::Error)?;
                Some(doc)
            }
            None => None,
        };

        // Determine log_id for this document. If this is the very first operation in the document
        // graph, the `document` value is None and we will return the next free log id
        let log = self
            .find_document_log_id(author, document)
            .await
            .map_err(|_| StorageProviderError::Error)?;

        // Determine backlink and skiplink hashes for the next entry. To do this we need the latest
        // entry in this log
        let entry_latest: Option<Self::Entry> = self
            .latest_entry(author, &log)
            .await
            .map_err(|_| StorageProviderError::Error)?;

        match entry_latest {
            // An entry was found which serves as the backlink for the upcoming entry
            Some(entry_backlink) => {
                let entry_hash_backlink = entry_backlink.entry_encoded().hash();
                let entry_latest: Self::Entry = self
                    .latest_entry(author, &log)
                    .await
                    .map_err(|_| StorageProviderError::Error)?
                    .unwrap();
                // Determine skiplink ("lipmaa"-link) entry in this log
                let entry_hash_skiplink = self
                    .determine_skiplink(&entry_latest)
                    .await
                    .map_err(|_| StorageProviderError::Error)?;

                Ok(Self::EntryArgsResponse::new(
                    Some(entry_hash_backlink),
                    entry_hash_skiplink,
                    entry_backlink.seq_num(),
                    entry_backlink.log_id(),
                ))
            }
            // No entry was given yet, we can assume this is the beginning of the log
            None => Ok(Self::EntryArgsResponse::new(
                None,
                None,
                SeqNum::default(),
                log,
            )),
        }
    }
}
