// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::entry::LogId;
use crate::hash::Hash;
use crate::identity::Author;
use crate::storage_provider::errors::LogStorageError;
use crate::storage_provider::traits::AsStorageLog;
use async_trait::async_trait;

/// Trait which handles all storage actions relating to `Log`s.
///
/// This trait should be implemented on the root storage provider struct. It's definitions
/// make up the required methods for inserting and querying logs from storage.
#[async_trait]
pub trait LogStore<StorageLog: AsStorageLog> {
    /// Insert a log into storage.
    async fn insert_log(&self, value: StorageLog) -> Result<bool, LogStorageError>;

    /// Get a log from storage
    async fn get(
        &self,
        author: &Author,
        document_id: &Hash,
    ) -> Result<Option<LogId>, LogStorageError>;

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
    ) -> Result<LogId, LogStorageError> {
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
    async fn next_log_id(&self, author: &Author) -> Result<LogId, LogStorageError>;
}
