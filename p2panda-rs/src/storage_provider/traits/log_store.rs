// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;

use crate::document::DocumentId;
use crate::entry::LogId;
use crate::identity::Author;
use crate::storage_provider::error::LogStorageError;
use crate::storage_provider::traits::AsStorageLog;

/// Trait which handles all storage actions relating to `StorageLog`s.
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
        document_id: &DocumentId,
    ) -> Result<Option<LogId>, LogStorageError>;

    /// Determines the next unused log_id of an author.
    async fn next_log_id(&self, author: &Author) -> Result<LogId, LogStorageError>;

    /// Determines the latest used log id for an author.
    ///
    /// Returns None when no log has been used yet.
    async fn latest_log_id(&self, author: &Author) -> Result<Option<LogId>, LogStorageError>;
}
