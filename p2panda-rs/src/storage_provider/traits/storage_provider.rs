// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;

use crate::document::DocumentId;
use crate::hash::Hash;
use crate::operation::traits::AsVerifiedOperation;
use crate::storage_provider::traits::{
    AsStorageLog, DocumentStore, EntryStore, EntryWithOperation, LogStore, OperationStore,
};
use crate::storage_provider::utils::Result;

/// Trait which handles all high level storage queries and insertions.
// @TODO: we no longer have any high level API methods living here, we can move
// `get_document_by_entry` somewhere else then this trait becomes a very simple wrapper
// encapsulating the storage traits required for the `domain` methods.
#[async_trait]
pub trait StorageProvider:
    EntryStore<Self::Entry>
    + LogStore<Self::StorageLog>
    + OperationStore<Self::Operation>
    + DocumentStore
{
    /// An associated type representing an entry as it passes in and out of storage.
    type Entry: EntryWithOperation;

    /// An associated type representing a log as it passes in and out of storage.
    type StorageLog: AsStorageLog;

    /// An associated type representing an operation as it passes in and out of storage.
    type Operation: AsVerifiedOperation;

    /// Returns the related document for any entry.
    ///
    /// Every entry is part of a document and, through that, associated with a specific log id used
    /// by this document and public key. This method returns that document id by looking up the log
    /// that the entry was stored in.
    ///
    /// If the passed entry cannot be found, or it's associated document doesn't exist yet, `None`
    /// is returned.
    async fn get_document_by_entry(&self, entry_hash: &Hash) -> Result<Option<DocumentId>>;
}
