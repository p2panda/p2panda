// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;

use crate::document::DocumentId;
use crate::entry::LogId;
use crate::identity::Author;
use crate::storage_provider::errors::LogStorageError;
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

    /// Returns registered or possible log id for a document.
    ///
    /// If no log has been previously registered for this document it
    /// automatically returns the next unused log_id.
    async fn find_document_log_id<'a>(
        &self,
        author: &Author,
        document_id: Option<&'a DocumentId>,
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
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::entry::LogId;
    use crate::identity::Author;
    use crate::storage_provider::traits::test_utils::{test_db, TestStore};
    use crate::storage_provider::traits::LogStore;

    #[rstest]
    #[tokio::test]
    async fn find_document_log_id(
        #[from(test_db)]
        #[with(3, 1, 1)]
        #[future]
        db: TestStore,
    ) {
        let db = db.await;

        let document_id = db.test_data.documents.get(0).unwrap();
        let key_pair = db.test_data.key_pairs.get(0).unwrap();
        let author = Author::from(key_pair.public_key());

        let log_id = db
            .store
            .find_document_log_id(&author, Some(document_id))
            .await
            .unwrap();
        assert_eq!(log_id, LogId::new(0));
    }
}
