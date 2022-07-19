// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;
use log::debug;

use crate::document::DocumentId;
use crate::entry::LogId;
use crate::identity::Author;
use crate::storage_provider::errors::LogStorageError;
use crate::storage_provider::traits::{AsStorageLog, LogStore};
use crate::test_utils::db::{MemoryStore, StorageLog};

/// Implement the `LogStore` trait on MemoryStore
#[async_trait]
impl LogStore<StorageLog> for MemoryStore {
    async fn insert_log(&self, log: StorageLog) -> Result<bool, LogStorageError> {
        debug!(
            "Inserting log {} into store for {}",
            log.id().as_u64(),
            log.author()
        );

        let author_log_id_str = log.author().as_str().to_string() + &log.id().as_u64().to_string();
        let mut logs = self.logs.lock().unwrap();
        logs.insert(author_log_id_str, log);
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
            .values()
            .find(|log| log.document_id() == *document_id && log.author() == *author);

        let log_id = log.map(|log| log.id());
        Ok(log_id)
    }

    async fn next_log_id(&self, author: &Author) -> Result<LogId, LogStorageError> {
        let logs = self.logs.lock().unwrap();

        let author_logs = logs.values().filter(|log| log.author() == *author);
        let next_log_id = author_logs.count();
        Ok(LogId::new(next_log_id as u64))
    }
    
    async fn latest_log_id(&self, author: &Author) -> Result<Option<LogId>, LogStorageError> {
        let logs = self.logs.lock().unwrap();

        let author_logs = logs.values().filter(|log| log.author() == *author);
        let log_count = author_logs.count();
        
        if log_count == 0 {Ok(None)} else {
            let latest_log_id = log_count -1;
            Ok(Some(LogId::new(latest_log_id as u64)))            
        }
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;

    use rstest::rstest;

    use crate::document::DocumentId;
    use crate::entry::LogId;
    use crate::identity::{Author, KeyPair};
    use crate::schema::SchemaId;
    use crate::storage_provider::traits::{AsStorageLog, LogStore};
    use crate::test_utils::db::{MemoryStore, StorageLog};
    use crate::test_utils::fixtures::{document_id, key_pair, schema};

    #[rstest]
    #[tokio::test]
    async fn insert_get_log(key_pair: KeyPair, schema: SchemaId, document_id: DocumentId) {
        // Instantiate a new store.
        let store = MemoryStore::default();

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
    #[tokio::test]
    async fn get_next_log_id(key_pair: KeyPair, schema: SchemaId, document_id: DocumentId) {
        // Instantiate a new store.
        let store = MemoryStore::default();

        let author = Author::try_from(key_pair.public_key().to_owned()).unwrap();
        let log_id = store.next_log_id(&author).await.unwrap();
        assert_eq!(log_id, LogId::default());

        let log = StorageLog::new(&author, &schema, &document_id, &LogId::default());

        assert!(store.insert_log(log).await.is_ok());

        let log_id = store.next_log_id(&author).await.unwrap();
        assert_eq!(log_id, LogId::new(1));
    }
}
