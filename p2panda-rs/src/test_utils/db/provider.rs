// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::document::{Document, DocumentId, DocumentView, DocumentViewId};
use crate::entry::LogId;
use crate::entry::traits::{AsEncodedEntry, AsEntry};
use crate::hash::Hash;
use crate::identity::PublicKey;
use crate::operation::OperationId;
use crate::schema::SchemaId;
use crate::storage_provider::traits::StorageProvider;
use crate::storage_provider::utils::Result;
use crate::test_utils::db::{PublishedOperation, StorageEntry};

type PublickeyLogId = String;
type Log = (PublicKey, LogId, SchemaId, DocumentId);

/// An in-memory implementation of p2panda storage provider traits.
///
/// Primarily used in testing environments.
#[derive(Default, Debug, Clone)]
pub struct MemoryStore {
    /// Stored logs
    pub logs: Arc<Mutex<HashMap<PublickeyLogId, Log>>>,

    /// Stored entries
    pub entries: Arc<Mutex<HashMap<Hash, StorageEntry>>>,

    /// Stored operations
    pub operations: Arc<Mutex<HashMap<OperationId, (DocumentId, PublishedOperation)>>>,

    /// Stored documents
    pub documents: Arc<Mutex<HashMap<DocumentId, Document>>>,

    /// Materialised and stored document views
    pub document_views: Arc<Mutex<HashMap<DocumentViewId, (SchemaId, DocumentView)>>>,
}

#[async_trait]
impl StorageProvider for MemoryStore {
    type Entry = StorageEntry;

    type Operation = PublishedOperation;

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

        let log = logs.iter().find(|(_, (public_key, log_id, _, _))| {
            log_id == entry.log_id() && public_key == entry.public_key()
        });

        Ok(log.map(|(_, (_, _, _, document_id))| document_id.to_owned()))
    }
}
