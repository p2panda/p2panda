// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::document::{Document, DocumentId, DocumentView, DocumentViewId};
use crate::hash::Hash;
use crate::operation::{OperationId, VerifiedOperation};
use crate::schema::SchemaId;
use crate::storage_provider::traits::StorageProvider;
use crate::storage_provider::traits::{AsStorageEntry, AsStorageLog};

use super::{
    EntryArgsRequest, EntryArgsResponse, PublishEntryRequest, PublishEntryResponse, StorageEntry,
    StorageLog,
};

type AuthorPlusLogId = String;
type DocumentViewIdStr = String;
type DocumentIdStr = String;

/// The simplest storage provider. Used for tests in `entry_store`, `log_store` & `storage_provider`
#[derive(Default, Debug)]
pub struct SimplestStorageProvider {
    pub logs: Arc<Mutex<BTreeMap<AuthorPlusLogId, StorageLog>>>,
    pub entries: Arc<Mutex<BTreeMap<Hash, StorageEntry>>>,
    pub operations: Arc<Mutex<BTreeMap<OperationId, (DocumentId, VerifiedOperation)>>>,
    pub documents: Arc<Mutex<BTreeMap<DocumentIdStr, Document>>>,
    pub document_views: Arc<Mutex<BTreeMap<DocumentViewIdStr, (SchemaId, DocumentView)>>>,
}

impl SimplestStorageProvider {
    pub fn db_insert_entry(&self, entry: StorageEntry) {
        let mut entries = self.entries.lock().unwrap();
        entries.insert(entry.hash(), entry);
    }

    pub fn db_insert_log(&self, log: StorageLog) {
        let author_log_id_str = log.author().as_str().to_string() + &log.id().as_u64().to_string();
        let mut logs = self.logs.lock().unwrap();
        logs.insert(author_log_id_str, log);
    }
}

#[async_trait]
impl StorageProvider<StorageEntry, StorageLog, VerifiedOperation> for SimplestStorageProvider {
    type EntryArgsRequest = EntryArgsRequest;

    type EntryArgsResponse = EntryArgsResponse;

    type PublishEntryRequest = PublishEntryRequest;

    type PublishEntryResponse = PublishEntryResponse;

    async fn get_document_by_entry(
        &self,
        entry_hash: &Hash,
    ) -> Result<Option<DocumentId>, Box<dyn std::error::Error + Sync + Send>> {
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
