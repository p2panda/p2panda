// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;
use bamboo_rs_core_ed25519_yasmf::entry::is_lipmaa_required;
use std::fmt::Debug;

use crate::document::DocumentId;
use crate::entry::{decode_entry, EntrySigned, SeqNum};
use crate::hash::Hash;
use crate::operation::{AsOperation, Operation, OperationEncoded};
use crate::Validate;
use crate::{entry::LogId, identity::Author};

use super::conversions::ToStorage;
use super::models::{AsEntry, AsLog};
use super::responses::AsEntryArgsResponse;
use super::StorageProviderError;

/// Trait which handles all storage actions relating to `Log`s.
#[async_trait]
pub trait LogStore<StorageLog: AsLog<MemoryLog> + Send, MemoryLog: ToStorage<StorageLog>> {
    /// The error type
    type LogError: Debug;

    /// Insert a log into storage.
    async fn insert_log(&self, value: StorageLog) -> Result<bool, Self::LogError>;

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
pub trait EntryStore<StorageEntry: AsEntry<MemoryEntry>, MemoryEntry: ToStorage<StorageEntry>> {
    /// The error type
    type EntryError: Debug;

    /// Insert an entry into storage.
    async fn insert_entry(&self, value: StorageEntry) -> Result<bool, Self::EntryError>;

    /// Returns entry at sequence position within an author's log.
    async fn entry_at_seq_num(
        &self,
        author: &Author,
        log_id: &LogId,
        seq_num: &SeqNum,
    ) -> Result<Option<StorageEntry>, Self::EntryError>;

    /// Returns the latest Bamboo entry of an author's log.
    async fn latest_entry(
        &self,
        author: &Author,
        log_id: &LogId,
    ) -> Result<Option<StorageEntry>, Self::EntryError>;

    /// Return vector of all entries of a given schema
    async fn by_schema(&self, schema: &Hash) -> Result<Vec<StorageEntry>, Self::EntryError>;

    /// Determine skiplink entry hash ("lipmaa"-link) for entry in this log, return `None` when no
    /// skiplink is required for the next entry.
    /// Determine skiplink entry hash ("lipmaa"-link) for entry in this log, return `None` when no
    /// skiplink is required for the next entry.
    async fn determine_skiplink(
        &self,
        entry: &StorageEntry,
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
pub trait StorageProvider<
    StorageEntry: AsEntry<MemoryEntry>,
    MemoryEntry: ToStorage<StorageEntry>,
    StorageLog: AsLog<MemoryLog> + Send,
    MemoryLog: ToStorage<StorageLog>,
>: EntryStore<StorageEntry, MemoryEntry> + LogStore<StorageLog, MemoryLog>
{
    /// The error type
    type Error: Debug + Send + Sync;
    type EntryArgsResponse: AsEntryArgsResponse + Send + Sync;
    type PublishEntryResponse: AsEntryArgsResponse + Send + Sync;
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
        let entry_latest: Option<StorageEntry> = self
            .latest_entry(author, &log)
            .await
            .map_err(|_| StorageProviderError::Error)?;

        match entry_latest {
            // An entry was found which serves as the backlink for the upcoming entry
            Some(entry_backlink) => {
                let entry_hash_backlink = entry_backlink.entry_encoded().hash();
                let entry_latest: StorageEntry = self
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

    /// Implementation of `panda_publishEntry` RPC method.
    ///
    /// Stores an author's Bamboo entry with operation payload in database after validating it.
    async fn publish_entry(
        &self,
        entry_encoded: EntrySigned,
        operation_encoded: OperationEncoded,
    ) -> Result<Self::PublishEntryResponse, StorageProviderError> {
        // Validate request parameters
        entry_encoded
            .validate()
            .map_err(|_| StorageProviderError::Error)?;
        operation_encoded
            .validate()
            .map_err(|_| StorageProviderError::Error)?;

        // Decode author, entry and operation. This conversion validates the operation hash
        let author = entry_encoded.author();
        let entry = decode_entry(&entry_encoded, Some(&operation_encoded))
            .map_err(|_| StorageProviderError::Error)?;
        let operation = Operation::from(&operation_encoded);

        // Every operation refers to a document we need to determine. A document is identified by the
        // hash of its first `CREATE` operation, it is the root operation of every document graph
        let document_id = if operation.is_create() {
            // This is easy: We just use the entry hash directly to determine the document id
            entry_encoded.hash()
        } else {
            // For any other operations which followed after creation we need to either walk the operation
            // graph back to its `CREATE` operation or more easily look up the database since we keep track
            // of all log ids and documents there.
            //
            // We can determine the used document hash by looking at what we know about the previous
            // entry in this author's log.
            //
            // @TODO: This currently looks at the backlink, in the future we want to use
            // "previousOperation", since in a multi-writer setting there might be no backlink for
            // update operations! See: https://github.com/p2panda/aquadoggo/issues/49
            let backlink_entry_hash = entry.backlink_hash().ok_or(StorageProviderError::Error)?;

            self.get_document_by_entry(backlink_entry_hash)
                .await
                .map_err(|_| StorageProviderError::Error)?
                .unwrap()
        };

        // Determine expected log id for new entry
        let document_log_id = self
            .find_document_log_id(&author, Some(&document_id))
            .await
            .map_err(|_| StorageProviderError::Error)?;

        // Check if provided log id matches expected log id
        if &document_log_id != entry.log_id() {
            return Err(StorageProviderError::Error);
        }

        // Get related bamboo backlink and skiplink entries
        let entry_backlink_bytes = if !entry.seq_num().is_first() {
            self.entry_at_seq_num(&author, entry.log_id(), &entry.seq_num_backlink().unwrap())
                .await
                .map_err(|_| StorageProviderError::Error)?
                .map(|link| {
                    let bytes = link.entry_encoded().to_bytes();
                    Some(bytes)
                })
                .ok_or(StorageProviderError::Error)
        } else {
            Ok(None)
        }?;

        let entry_skiplink_bytes = if !entry.seq_num().is_first() {
            self.entry_at_seq_num(&author, entry.log_id(), &entry.seq_num_skiplink().unwrap())
                .await
                .map_err(|_| StorageProviderError::Error)?
                .map(|link| {
                    let bytes = link.entry_encoded().to_bytes();
                    Some(bytes)
                })
                .ok_or(StorageProviderError::Error)
        } else {
            Ok(None)
        }?;

        // Verify bamboo entry integrity, including encoding, signature of the entry correct back- and
        // skiplinks.
        bamboo_rs_core_ed25519_yasmf::verify(
            &entry_encoded.to_bytes(),
            Some(&operation_encoded.to_bytes()),
            entry_skiplink_bytes.as_deref(),
            entry_backlink_bytes.as_deref(),
        )
        .map_err(|_| StorageProviderError::Error)?;

        // Register log in database when a new document is created
        if operation.is_create() {
            let log = StorageLog::new(
                author.clone(),
                DocumentId::new(document_id),
                operation.schema(),
                *entry.log_id(),
            );
            self.insert_log(log)
                .await
                .map_err(|_| StorageProviderError::Error)?;
        }

        // Finally insert Entry in database
        self.insert_entry(StorageEntry::new(
            entry_encoded.clone(),
            Some(operation_encoded),
        ))
        .await
        .map_err(|_| StorageProviderError::Error)?;

        // Already return arguments for next entry creation
        let entry_latest: StorageEntry = self
            .latest_entry(&author, entry.log_id())
            .await
            .map_err(|_| StorageProviderError::Error)?
            .unwrap();
        let entry_hash_skiplink = self
            .determine_skiplink(&entry_latest)
            .await
            .map_err(|_| StorageProviderError::Error)?;
        let next_seq_num = entry_latest.seq_num().next().unwrap();

        Ok(Self::PublishEntryResponse::new(
            Some(entry_encoded.hash()),
            entry_hash_skiplink,
            next_seq_num,
            *entry.log_id(),
        ))
    }
}
