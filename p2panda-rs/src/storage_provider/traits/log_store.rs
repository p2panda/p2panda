// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::document::DocumentId;
use crate::entry::LogId;
use crate::identity::Author;
use crate::storage_provider::errors::LogStorageError;
use crate::storage_provider::traits::AsStorageLog;
use async_trait::async_trait;

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

    /// Returns registered or possible log id for a document.
    ///
    /// If no log has been previously registered for this document it
    /// automatically returns the next unused log_id.
    async fn find_document_log_id(
        &self,
        author: &Author,
        document_id: Option<&DocumentId>,
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
pub mod tests {
    use std::convert::TryFrom;

    use async_trait::async_trait;
    use rstest::rstest;
    use std::sync::{Arc, Mutex};

    use crate::document::DocumentId;
    use crate::entry::LogId;
    use crate::identity::{Author, KeyPair};
    use crate::schema::SchemaId;
    use crate::storage_provider::errors::LogStorageError;
    use crate::storage_provider::models::Log;
    use crate::storage_provider::traits::test_utils::{SimplestStorageProvider, StorageLog};
    use crate::storage_provider::traits::{AsStorageLog, LogStore};
    use crate::test_utils::fixtures::{document_id, key_pair, schema};

    /// Implement the `LogStore` trait on SimplestStorageProvider
    #[async_trait]
    impl LogStore<StorageLog> for SimplestStorageProvider {
        async fn insert_log(&self, log: StorageLog) -> Result<bool, LogStorageError> {
            let mut logs = self.logs.lock().unwrap();
            logs.push(log);
            // Remove duplicate log entries.
            logs.dedup();
            Ok(true)
        }

        /// Get a log from storage
        async fn get(
            &self,
            author: &Author,
            document_id: &DocumentId,
        ) -> Result<Option<LogId>, LogStorageError> {
            let logs = self.logs.lock().unwrap();

            let log = logs
                .iter()
                .find(|log| log.document() == *document_id && log.author() == *author);

            let log_id = log.map(|log| log.log_id());
            Ok(log_id)
        }

        async fn next_log_id(&self, author: &Author) -> Result<LogId, LogStorageError> {
            let logs = self.logs.lock().unwrap();

            let author_logs = logs.iter().filter(|log| log.author() == *author);
            let next_log_id = author_logs.count() + 1;
            Ok(LogId::new(next_log_id as u64))
        }
    }

    #[rstest]
    #[async_std::test]
    async fn insert_get_log(key_pair: KeyPair, schema: SchemaId, document_id: DocumentId) {
        // Instantiate a new store.
        let store = SimplestStorageProvider {
            logs: Arc::new(Mutex::new(Vec::new())),
            entries: Arc::new(Mutex::new(Vec::new())),
        };

        let author = Author::try_from(key_pair.public_key().to_owned()).unwrap();
        let log = StorageLog::new(Log::new(&author, &schema, &document_id, &LogId::default()));

        // Insert a log into the store.
        assert!(store.insert_log(log).await.is_ok());

        // Get a log_id from the store by author and document_id.
        let log_id = store.get(&author, &document_id).await;

        assert!(log_id.is_ok());
        assert_eq!(log_id.unwrap().unwrap(), LogId::default())
    }

    #[rstest]
    #[async_std::test]
    async fn get_next_log_id(key_pair: KeyPair, schema: SchemaId, document_id: DocumentId) {
        // Instantiate a new store.
        let store = SimplestStorageProvider {
            logs: Arc::new(Mutex::new(Vec::new())),
            entries: Arc::new(Mutex::new(Vec::new())),
        };

        let author = Author::try_from(key_pair.public_key().to_owned()).unwrap();
        let log_id = store.next_log_id(&author).await.unwrap();
        assert_eq!(log_id, LogId::default());

        let log = Log::new(&author, &schema, &document_id, &LogId::default()).into();

        assert!(store.insert_log(log).await.is_ok());

        let log_id = store.next_log_id(&author).await.unwrap();
        assert_eq!(log_id, LogId::new(2));
    }
}
