use std::convert::{TryFrom, TryInto};
// SPDX-License-Identifier: AGPL-3.0-or-later
use std::fmt::Debug;

use crate::document::DocumentId;
use crate::entry::{decode_entry, Entry, EntrySigned, LogId, SeqNum};
use crate::identity::Author;
use crate::operation::OperationEncoded;
use crate::schema::SchemaId;
use crate::storage_provider::models::EntryWithOperation;

pub trait AsStorageEntry:
    Sized + Clone + Send + Sync + TryInto<EntryWithOperation> + TryFrom<EntryWithOperation>
{
    type AsStorageEntryError: Debug;

    fn new(entry_with_operation: EntryWithOperation) -> Result<Self, Self::AsStorageEntryError>;

    fn entry_encoded(&self) -> EntrySigned;

    fn operation_encoded(&self) -> Option<OperationEncoded>;

    fn entry_decoded(&self) -> Entry {
        // Unwrapping optimistically for now...
        decode_entry(&self.entry_encoded(), self.operation_encoded().as_ref()).unwrap()
    }
}

pub trait AsStorageLog: Sized + Send + Sync {
    type AsLogError: Debug;

    fn new(author: Author, document: DocumentId, schema: SchemaId, log_id: LogId) -> Self;
    fn author(&self) -> Author;
    fn log_id(&self) -> LogId;
    fn document(&self) -> DocumentId;
    fn schema(&self) -> SchemaId;
}
