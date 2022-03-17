// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::{TryFrom, TryInto};
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use crate::document::DocumentId;
use crate::entry::{EntrySigned, LogId};
use crate::identity::Author;
use crate::operation::OperationEncoded;
use crate::schema::SchemaId;
use crate::storage_provider::errors::EntryStorageError;
use crate::storage_provider::models::EntryWithOperation;
use crate::storage_provider::traits::{AsStorageEntry, AsStorageLog};

/// The simplest storage provider. Used for tests in `entry_store`, `log_store` & `storage_provider`
pub struct SimplestStorageProvider {
    pub logs: Arc<Mutex<Vec<StorageLog>>>,
    pub entries: Arc<Mutex<Vec<StorageEntry>>>,
}

/// A log entry represented as a concatenated string of `"{author}-{schema}-{document_id}-{log_id}"`
#[derive(Debug, Clone, PartialEq)]
pub struct StorageLog(String);

/// Implement `AsStorageLog` trait for our `StorageLog` struct
impl AsStorageLog for StorageLog {
    fn new(author: &Author, document: &DocumentId, schema: &SchemaId, log_id: &LogId) -> Self {
        // Convert SchemaId into a string
        let schema_id = match schema.clone() {
            SchemaId::Application(pinned_relation) => {
                let mut id_str = "".to_string();
                let mut relation_iter = pinned_relation.into_iter().peekable();
                while let Some(hash) = relation_iter.next() {
                    id_str += hash.as_str();
                    if relation_iter.peek().is_none() {
                        id_str += "_"
                    }
                }
                id_str
            }
            SchemaId::Schema => "schema_v1".to_string(),
            SchemaId::SchemaField => "schema_field_v1".to_string(),
        };

        // Concat all values
        let log_string = format!(
            "{}-{}-{}-{}",
            author.as_str(),
            schema_id,
            document.as_str(),
            log_id.as_u64()
        );

        Self(log_string)
    }

    fn author(&self) -> Author {
        let params: Vec<&str> = self.0.split('-').collect();
        Author::new(params[0]).unwrap()
    }

    fn schema(&self) -> SchemaId {
        let params: Vec<&str> = self.0.split('-').collect();
        SchemaId::from_str(params[1]).unwrap()
    }

    fn document(&self) -> DocumentId {
        let params: Vec<&str> = self.0.split('-').collect();
        DocumentId::from_str(params[2]).unwrap()
    }

    fn log_id(&self) -> LogId {
        let params: Vec<&str> = self.0.split('-').collect();
        LogId::from_str(params[3]).unwrap()
    }
}

/// A struct which represents an entry and operation pair in storage as a concatenated string.
#[derive(Debug, Clone, PartialEq)]
pub struct StorageEntry(String);

impl StorageEntry {
    pub fn new(entry_encoded: EntrySigned, operation_encoded: OperationEncoded) -> Self {
        // Concat all values
        let log_string = format!("{}-{}", entry_encoded.as_str(), operation_encoded.as_str());

        Self(log_string)
    }
}

/// Implement `AsStorageEntry` trait for `StorageEntry`
impl AsStorageEntry for StorageEntry {
    type AsStorageEntryError = EntryStorageError;

    fn entry_encoded(&self) -> EntrySigned {
        let params: Vec<&str> = self.0.split('-').collect();
        EntrySigned::new(params[0]).unwrap()
    }

    fn operation_encoded(&self) -> Option<OperationEncoded> {
        let params: Vec<&str> = self.0.split('-').collect();
        Some(OperationEncoded::new(params[1]).unwrap())
    }
}

/// Implement required `TryFrom` conversion trait.
impl TryFrom<EntryWithOperation> for StorageEntry {
    type Error = EntryStorageError;

    fn try_from(value: EntryWithOperation) -> Result<Self, Self::Error> {
        Ok(StorageEntry::new(
            value.entry_encoded().to_owned(),
            value.operation_encoded().to_owned(),
        ))
    }
}

/// Implement required `TryInto` conversion trait.
impl TryInto<EntryWithOperation> for StorageEntry {
    type Error = EntryStorageError;

    fn try_into(self) -> Result<EntryWithOperation, Self::Error> {
        Ok(
            EntryWithOperation::new(self.entry_encoded(), self.operation_encoded().unwrap())
                .unwrap(),
        )
    }
}
