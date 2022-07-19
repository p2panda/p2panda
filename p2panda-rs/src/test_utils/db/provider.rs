// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::document::{Document, DocumentId, DocumentView, DocumentViewId};
use crate::hash::Hash;
use crate::operation::{OperationId, VerifiedOperation};
use crate::schema::SchemaId;
use crate::storage_provider::traits::StorageProvider;
use crate::storage_provider::traits::{AsStorageEntry, AsStorageLog};
use crate::storage_provider::utils::Result;
use crate::test_utils::db::{StorageEntry, StorageLog};

type AuthorPlusLogId = String;

/// An in-memory implementation of p2panda storage provider traits.
///
/// Primarily used in testing environments.
#[derive(Default, Debug, Clone)]
pub struct MemoryStore {
    /// Stored logs
    pub logs: Arc<Mutex<HashMap<AuthorPlusLogId, StorageLog>>>,

    /// Stored entries
    pub entries: Arc<Mutex<HashMap<Hash, StorageEntry>>>,

    /// Stored operations
    pub operations: Arc<Mutex<HashMap<OperationId, (DocumentId, VerifiedOperation)>>>,

    /// Stored documents
    pub documents: Arc<Mutex<HashMap<DocumentId, Document>>>,

    /// Materialised and stored document views
    pub document_views: Arc<Mutex<HashMap<DocumentViewId, (SchemaId, DocumentView)>>>,
}

#[async_trait]
impl StorageProvider for MemoryStore {
    
    type StorageEntry = StorageEntry;

    type StorageLog = StorageLog;

    type StorageOperation = VerifiedOperation;

    async fn get_document_by_entry(&self, entry_hash: &Hash) -> Result<Option<DocumentId>> {
        let entries = self.entries.lock().unwrap();

        let entry = entries
            .iter()
            .find(|(_, entry)| entry.hash() == *entry_hash);

        let entry = match entry {
            Some((_, entry)) => entry,
            None => return Ok(None),
        };

        let logs = self.logs.lock().unwrap();

        let log = logs
            .iter()
            .find(|(_, log)| log.id() == entry.log_id() && log.author() == entry.author());

        Ok(log.map(|(_, log)| log.document_id()))
    }
}
