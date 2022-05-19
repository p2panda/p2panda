// SPDX-License-Identifier: AGPL-3.0-or-later

use std::str::FromStr;

use async_trait::async_trait;

use crate::document::DocumentId;
use crate::entry::LogId;
use crate::identity::Author;
use crate::schema::SchemaId;
use crate::storage_provider::log::{AsStorageLog, LogStorageError, LogStore};
use crate::storage_provider::test_provider::SimplestStorageProvider;

/// A log entry represented as a concatenated string of `"{author}-{schema}-{document_id}-{log_id}"`
#[derive(Debug, Clone, PartialEq)]
pub struct StorageLog(String);

/// Implement `AsStorageLog` trait for our `StorageLog` struct
impl AsStorageLog for StorageLog {
    fn new(author: &Author, schema: &SchemaId, document: &DocumentId, log_id: &LogId) -> Self {
        // Concat all values
        let log_string = format!(
            "{}-{}-{}-{}",
            author.as_str(),
            schema.as_str(),
            document.as_str(),
            log_id.as_u64()
        );

        Self(log_string)
    }

    fn author(&self) -> Author {
        let params: Vec<&str> = self.0.split('-').collect();
        Author::new(params[0]).unwrap()
    }

    fn schema_id(&self) -> SchemaId {
        let params: Vec<&str> = self.0.split('-').collect();
        SchemaId::from_str(params[1]).unwrap()
    }

    fn document_id(&self) -> DocumentId {
        let params: Vec<&str> = self.0.split('-').collect();
        DocumentId::from_str(params[2]).unwrap()
    }

    fn id(&self) -> LogId {
        let params: Vec<&str> = self.0.split('-').collect();
        LogId::from_str(params[3]).unwrap()
    }
}

/// Implement the `LogStore` trait on SimplestStorageProvider
#[async_trait]
impl LogStore<StorageLog> for SimplestStorageProvider {
    async fn insert_log(&self, log: StorageLog) -> Result<bool, LogStorageError> {
        self.db_insert_log(log);
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
            .find(|log| log.document_id() == *document_id && log.author() == *author);

        let log_id = log.map(|log| log.id());
        Ok(log_id)
    }

    async fn next_log_id(&self, author: &Author) -> Result<LogId, LogStorageError> {
        let logs = self.logs.lock().unwrap();

        let author_logs = logs.iter().filter(|log| log.author() == *author);
        let next_log_id = author_logs.count() + 1;
        Ok(LogId::new(next_log_id as u64))
    }
}

#[cfg(test)]
pub mod tests {
    use std::convert::TryFrom;
    use std::sync::{Arc, Mutex};

    use rstest::rstest;

    use crate::document::DocumentId;
    use crate::entry::LogId;
    use crate::identity::{Author, KeyPair};
    use crate::schema::SchemaId;
    use crate::storage_provider::log::{AsStorageLog, LogStore};
    use crate::storage_provider::test_provider::{SimplestStorageProvider, StorageLog};
    use crate::test_utils::fixtures::{document_id, key_pair, schema};

    #[rstest]
    #[async_std::test]
    async fn insert_get_log(key_pair: KeyPair, schema: SchemaId, document_id: DocumentId) {
        // Instantiate a new store.
        let store = SimplestStorageProvider {
            logs: Arc::new(Mutex::new(Vec::new())),
            entries: Arc::new(Mutex::new(Vec::new())),
        };

        let author = Author::try_from(key_pair.public_key().to_owned()).unwrap();
        let log = StorageLog::new(&author, &schema, &document_id, &LogId::default());

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

        let log = StorageLog::new(&author, &schema, &document_id, &LogId::default());

        assert!(store.insert_log(log).await.is_ok());

        let log_id = store.next_log_id(&author).await.unwrap();
        assert_eq!(log_id, LogId::new(2));
    }
}