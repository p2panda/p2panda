// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;

use crate::document::DocumentId;
use crate::entry::traits::{AsEncodedEntry, AsEntry};
use crate::hash::Hash;
use crate::operation::traits::AsVerifiedOperation;
use crate::storage_provider::traits::{AsStorageLog, EntryStore, LogStore, OperationStore};
use crate::storage_provider::utils::Result;

use super::EntryWithOperation;

/// Trait which handles all high level storage queries and insertions.
// @TODO: we no longer have any high level API methods living here, we can move
// `get_document_by_entry` somewhere else then this trait becomes a very simple wrapper
// encapsulating the storage traits required for the `domain` methods.
#[async_trait]
pub trait StorageProvider:
    EntryStore<Self::Entry> + LogStore<Self::StorageLog> + OperationStore<Self::StorageOperation>
{
    // TODO: We can move these types into their own stores once we deprecate the
    // higher level methods (publish_entry and next_entry_args) on StorageProvider.

    /// An associated type representing an entry as it passes in and out of storage.
    type Entry: EntryWithOperation;

    /// An associated type representing a log as it passes in and out of storage.
    type StorageLog: AsStorageLog;

    /// An associated type representing an operation as it passes in and out of storage.
    type StorageOperation: AsVerifiedOperation;

    /// Returns the related document for any entry.
    ///
    /// Every entry is part of a document and, through that, associated with a specific log id used
    /// by this document and author. This method returns that document id by looking up the log
    /// that the entry was stored in.
    ///
    /// If the passed entry cannot be found, or it's associated document doesn't exist yet, `None`
    /// is returned.
    async fn get_document_by_entry(&self, entry_hash: &Hash) -> Result<Option<DocumentId>>;
}
