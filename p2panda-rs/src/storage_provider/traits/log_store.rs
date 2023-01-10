// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;

use crate::document::DocumentId;
use crate::entry::LogId;
use crate::identity::PublicKey;
use crate::schema::SchemaId;
use crate::storage_provider::error::LogStorageError;

/// Storage interface for inserting and querying `Entries`.
///
/// Logs are derived from the `Entries` which arrive at and are stored on a node. These methods 
/// should be used to store new logs when so needed and then to perform queries on the stored data.
/// 
/// Each log, as well as all `Entries` and `Operations` it contains, is associated with exactly one 
/// `PublicKey`, `SchemaId` and `DocumentId`.
#[async_trait]
pub trait LogStore {
    /// Insert a log into the store.
    /// 
    /// 
    async fn insert_log(
        &self,
        log_id: &LogId,
        public_key: &PublicKey,
        schema: &SchemaId,
        document: &DocumentId,
    ) -> Result<bool, LogStorageError>;

    /// Get the log id for a `PublicKey` and `DocumentId`.
    ///
    /// Returns a `LogId` or `None` if no log exists with the passed `PublicKey` and `DocumentId`.
    async fn get_log_id(
        &self,
        public_key: &PublicKey,
        document_id: &DocumentId,
    ) -> Result<Option<LogId>, LogStorageError>;

    /// Determines the latest used `LogId` for a `PublicKey`.
    ///
    /// Returns a `LogId` or `None` if the passed `PublicKey` has not published any entry yet.
    async fn latest_log_id(&self, public_key: &PublicKey)
        -> Result<Option<LogId>, LogStorageError>;
}
