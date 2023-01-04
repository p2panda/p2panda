// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;
use log::debug;

use crate::document::DocumentId;
use crate::entry::LogId;
use crate::identity::PublicKey;
use crate::schema::SchemaId;
use crate::storage_provider::error::LogStorageError;
use crate::storage_provider::traits::LogStore;
use crate::test_utils::memory_store::MemoryStore;

/// Implement the `LogStore` trait on MemoryStore
#[async_trait]
impl LogStore for MemoryStore {
    async fn insert_log(
        &self,
        log_id: &LogId,
        public_key: &PublicKey,
        schema: &SchemaId,
        document: &DocumentId,
    ) -> Result<bool, LogStorageError> {
        debug!(
            "Inserting log {} into store for {}",
            log_id.as_u64(),
            public_key
        );

        let public_key_log_id_str = public_key.to_string() + &log_id.as_u64().to_string();
        let mut logs = self.logs.lock().unwrap();
        logs.insert(
            public_key_log_id_str,
            (*public_key, *log_id, schema.to_owned(), document.to_owned()),
        );
        Ok(true)
    }

    /// Get a log from storage
    async fn get_log_id(
        &self,
        public_key: &PublicKey,
        document_id: &DocumentId,
    ) -> Result<Option<LogId>, LogStorageError> {
        let logs = self.logs.lock().unwrap();

        let log = logs
            .values()
            .find(|(pk, _, _, doc_id)| doc_id == document_id && pk == public_key);

        let log_id = log.map(|(_, log_id, _, _)| log_id);
        Ok(log_id.cloned())
    }

    async fn latest_log_id(
        &self,
        public_key: &PublicKey,
    ) -> Result<Option<LogId>, LogStorageError> {
        let logs = self.logs.lock().unwrap();

        let public_key_logs = logs.values().filter(|(pk, _, _, _)| pk == public_key);
        let log_count = public_key_logs.count();

        if log_count == 0 {
            Ok(None)
        } else {
            let latest_log_id = log_count - 1;
            Ok(Some(LogId::new(latest_log_id as u64)))
        }
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::document::DocumentId;
    use crate::entry::LogId;
    use crate::identity::KeyPair;
    use crate::schema::SchemaId;
    use crate::storage_provider::traits::LogStore;
    use crate::test_utils::memory_store::MemoryStore;
    use crate::test_utils::fixtures::{document_id, key_pair, schema_id};

    #[rstest]
    #[tokio::test]
    async fn insert_get_log(key_pair: KeyPair, schema_id: SchemaId, document_id: DocumentId) {
        // Instantiate a new store.
        let store = MemoryStore::default();

        let public_key = key_pair.public_key();

        // Insert a log into the store.
        assert!(store
            .insert_log(&LogId::default(), &public_key, &schema_id, &document_id)
            .await
            .is_ok());

        // Get a log_id from the store by public_key and document_id.
        let log_id = store.get_log_id(&public_key, &document_id).await;

        assert!(log_id.is_ok());
        assert_eq!(log_id.unwrap().unwrap(), LogId::default())
    }

    #[rstest]
    #[tokio::test]
    async fn get_latest_log_id(key_pair: KeyPair, schema_id: SchemaId, document_id: DocumentId) {
        // Instantiate a new store.
        let store = MemoryStore::default();

        let public_key = key_pair.public_key();
        let log_id = store.latest_log_id(&public_key).await.unwrap();
        assert_eq!(log_id, None);

        assert!(store
            .insert_log(&LogId::default(), &public_key, &schema_id, &document_id)
            .await
            .is_ok());

        let log_id = store.latest_log_id(&public_key).await.unwrap();
        assert_eq!(log_id, Some(LogId::default()));
    }
}
