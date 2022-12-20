// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;

use crate::document::DocumentId;
use crate::entry::LogId;
use crate::identity::PublicKey;
use crate::schema::SchemaId;
use crate::storage_provider::error::LogStorageError;

/// Trait which defines storage actions relating to logs.
///
/// This trait should be implemented on the root storage provider struct. It's definitions
/// make up the required methods for inserting and querying logs from storage.
#[async_trait]
pub trait LogStore {
    /// Insert a log into storage.
    async fn insert_log(
        &self,
        log_id: &LogId,
        public_key: &PublicKey,
        schema: &SchemaId,
        document: &DocumentId,
    ) -> Result<bool, LogStorageError>;

    /// Get a log from storage
    async fn get(
        &self,
        public_key: &PublicKey,
        document_id: &DocumentId,
    ) -> Result<Option<LogId>, LogStorageError>;

    /// Determines the next unused log id for a public key.
    async fn next_log_id(&self, public_key: &PublicKey) -> Result<LogId, LogStorageError>;

    /// Determines the latest used log id for a public key.
    ///
    /// Returns None when no log has been used yet.
    async fn latest_log_id(&self, public_key: &PublicKey)
        -> Result<Option<LogId>, LogStorageError>;
}
