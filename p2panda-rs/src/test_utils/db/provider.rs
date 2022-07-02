// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::document::DocumentId;
use crate::hash::Hash;
use crate::operation::VerifiedOperation;
use crate::storage_provider::traits::StorageProvider;
use crate::storage_provider::traits::{AsStorageEntry, AsStorageLog};

use super::{
    EntryArgsRequest, EntryArgsResponse, PublishEntryRequest, PublishEntryResponse, StorageEntry,
    StorageLog,
};

/// The simplest storage provider. Used for tests in `entry_store`, `log_store` & `storage_provider`
#[derive(Default)]
pub struct SimplestStorageProvider {
    pub logs: Arc<Mutex<Vec<StorageLog>>>,
    pub entries: Arc<Mutex<Vec<StorageEntry>>>,
    pub operations: Arc<Mutex<Vec<(DocumentId, VerifiedOperation)>>>,
}

impl SimplestStorageProvider {
    pub fn db_insert_entry(&self, entry: StorageEntry) {
        let mut entries = self.entries.lock().unwrap();
        entries.push(entry);
        // Remove duplicate entries.
        entries.dedup();
    }

    pub fn db_insert_log(&self, log: StorageLog) {
        let mut logs = self.logs.lock().unwrap();
        logs.push(log);
        // Remove duplicate logs.
        logs.dedup();
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

        let entry = entries.iter().find(|entry| entry.hash() == *entry_hash);

        let entry = match entry {
            Some(entry) => entry,
            None => return Ok(None),
        };

        let logs = self.logs.lock().unwrap();

        let log = logs
            .iter()
            .find(|log| log.id() == entry.log_id() && log.author() == entry.author());

        Ok(Some(log.unwrap().document_id()))
    }
}
