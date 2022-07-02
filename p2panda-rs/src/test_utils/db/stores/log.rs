use async_trait::async_trait;

use crate::document::DocumentId;
use crate::entry::LogId;
use crate::identity::Author;
use crate::storage_provider::errors::LogStorageError;
use crate::storage_provider::traits::test_utils::{SimplestStorageProvider, StorageLog};
use crate::storage_provider::traits::{AsStorageLog, LogStore};

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

    use rstest::rstest;

    use crate::document::DocumentId;
    use crate::entry::LogId;
    use crate::identity::{Author, KeyPair};
    use crate::schema::SchemaId;
    use crate::storage_provider::traits::test_utils::{
        test_db, SimplestStorageProvider, StorageLog, TestStore,
    };
    use crate::storage_provider::traits::{AsStorageLog, LogStore};
    use crate::test_utils::fixtures::{document_id, key_pair, schema};

    #[rstest]
    #[async_std::test]
    async fn insert_get_log(key_pair: KeyPair, schema: SchemaId, document_id: DocumentId) {
        // Instantiate a new store.
        let store = SimplestStorageProvider::default();

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
        let store = SimplestStorageProvider::default();

        let author = Author::try_from(key_pair.public_key().to_owned()).unwrap();
        let log_id = store.next_log_id(&author).await.unwrap();
        assert_eq!(log_id, LogId::default());

        let log = StorageLog::new(&author, &schema, &document_id, &LogId::default());

        assert!(store.insert_log(log).await.is_ok());

        let log_id = store.next_log_id(&author).await.unwrap();
        assert_eq!(log_id, LogId::new(2));
    }

    #[rstest]
    #[async_std::test]
    async fn find_document_log_id(
        #[from(test_db)]
        #[with(3, 1)]
        #[future]
        db: TestStore,
    ) {
        let db = db.await;
        let document_id = db.documents.get(0).unwrap();
        let key_pair = db.key_pairs.get(0).unwrap();
        let author = Author::try_from(key_pair.public_key().to_owned()).unwrap();

        let log_id = db
            .store
            .find_document_log_id(&author, Some(document_id))
            .await
            .unwrap();
        assert_eq!(log_id, LogId::new(1));
    }
}
