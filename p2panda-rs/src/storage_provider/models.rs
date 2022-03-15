use std::convert::{TryFrom, TryInto};
// SPDX-License-Identifier: AGPL-3.0-or-later
use std::fmt::Debug;

use super::StorageProviderError;
use crate::document::DocumentId;
use crate::entry::{decode_entry, Entry, EntrySigned, LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::Author;
use crate::operation::OperationEncoded;
use crate::schema::SchemaId;

#[derive(Debug, Clone)]
pub struct EntryWithOperation(EntrySigned, Option<OperationEncoded>);

impl EntryWithOperation {
    pub fn new(
        entry: EntrySigned,
        operation: Option<OperationEncoded>,
    ) -> Result<Self, StorageProviderError> {
        // TODO: Validate entry + operation here

        Ok(Self(entry, operation))
    }
    pub fn entry_encoded(&self) -> &EntrySigned {
        &self.0
    }
    pub fn operation_encoded(&self) -> Option<&OperationEncoded> {
        self.1.as_ref()
    }
}

/// Trait required for entries which will pass in and out of storage.
pub trait AsStorageEntry:
    Sized + Clone + Send + Sync + TryInto<EntryWithOperation> + TryFrom<EntryWithOperation>
{
    type AsStorageEntryError: Debug;

    fn new(entry_with_operation: EntryWithOperation) -> Result<Self, Self::AsStorageEntryError>;

    fn entry_encoded(&self) -> EntrySigned;

    fn operation_encoded(&self) -> Option<OperationEncoded>;

    fn entry(&self) -> Entry {
        // Unwrapping optimistically for now...
        decode_entry(&self.entry_encoded(), self.operation_encoded().as_ref()).unwrap()
    }
}

pub trait AsLog: Sized + Send + Sync {
    type AsLogError: Debug;

    fn new(author: Author, document: DocumentId, schema: SchemaId, log_id: LogId) -> Self;
    fn author(&self) -> Author;
    fn log_id(&self) -> LogId;
    fn document(&self) -> DocumentId;
    fn schema(&self) -> SchemaId;
}
