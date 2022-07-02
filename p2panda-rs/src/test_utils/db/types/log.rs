// SPDX-License-Identifier: AGPL-3.0-or-later

use std::str::FromStr;

use crate::document::DocumentId;
use crate::entry::LogId;
use crate::identity::Author;
use crate::schema::SchemaId;
use crate::storage_provider::traits::AsStorageLog;

/// A log entry represented as a concatenated string of `"{author}-{schema}-{document_id}-{log_id}"`
#[derive(Debug, Clone, PartialEq)]
pub struct StorageLog(String);

/// Implement `AsStorageLog` trait for our `StorageLog` struct
impl AsStorageLog for StorageLog {
    fn new(author: &Author, schema: &SchemaId, document: &DocumentId, log_id: &LogId) -> Self {
        // Concat all values
        let log_string = format!(
            "{}-{}-{}-{}",
            author.as_str(),
            schema.as_str(),
            document.as_str(),
            log_id.as_u64()
        );

        Self(log_string)
    }

    fn author(&self) -> Author {
        let params: Vec<&str> = self.0.split('-').collect();
        Author::new(params[0]).unwrap()
    }

    fn schema_id(&self) -> SchemaId {
        let params: Vec<&str> = self.0.split('-').collect();
        SchemaId::from_str(params[1]).unwrap()
    }

    fn document_id(&self) -> DocumentId {
        let params: Vec<&str> = self.0.split('-').collect();
        DocumentId::from_str(params[2]).unwrap()
    }

    fn id(&self) -> LogId {
        let params: Vec<&str> = self.0.split('-').collect();
        LogId::from_str(params[3]).unwrap()
    }
}
