// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;

use crate::next::document::DocumentId;
use crate::next::hash::Hash;
use crate::next::operation::traits::AsVerifiedOperation;
use crate::storage_provider::traits::{
    AsStorageEntry, AsStorageLog, EntryStore, LogStore, OperationStore,
};
use crate::storage_provider::utils::Result;

/// Trait which handles all high level storage queries and insertions.
///
/// This trait should be implemented on the root storage provider struct. It's definitions make up
/// the high level methods a p2panda client needs when interacting with data storage. It will be
/// used for storing entries (`publish_entry`), getting required entry arguments when creating
/// entries (`get_entry_args`) and all internal storage actions. Methods defined on `EntryStore`
/// and `LogStore` and `OperationStore` are for lower level access to their respective data
/// structures.
///
/// The methods defined here are the minimum required for a working storage backend, additional
/// custom methods can be added per implementation.
///
/// For example: if I wanted to use an SQLite backend, then I would first implement [`LogStore`]
/// and [`EntryStore`] traits with all their required methods defined (they are required traits
/// containing lower level accessors and setters for the respective data structures). With these
/// traits defined [`StorageProvider`] is almost complete as it contains default definitions for
/// most of it's methods (`get_entry_args` and `publish_entry` are defined below). The only one
/// which needs defining is `get_document_by_entry`. It is also possible to over-ride the default
/// definitions for any of the trait methods.
#[async_trait]
pub trait StorageProvider:
    EntryStore<Self::StorageEntry> + LogStore<Self::StorageLog> + OperationStore<Self::StorageOperation>
{
    // TODO: We can move these types into their own stores once we deprecate the
    // higher level methods (publish_entry and next_entry_args) on StorageProvider.

    /// An associated type representing an entry as it passes in and out of storage.
    type StorageEntry: AsStorageEntry;

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
