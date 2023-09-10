// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;

use crate::entry::traits::{AsEncodedEntry, AsEntry};
use crate::entry::SeqNum;
use crate::entry::{EncodedEntry, Entry as P2pandaEntry, LogId};
use crate::hash::Hash;
use crate::identity::PublicKey;
use crate::operation::EncodedOperation;
use crate::storage_provider::error::EntryStorageError;

/// Storage interface for storing and querying `Entries`.
///
/// `Entries` are a core data type of p2panda, they form an append-only `Bamboo` log structure and carry
/// an `Operation` as their deletable payload.
///
/// Where a method takes several parameters it is assumed that passed values have the expected relationship
/// and any required validation has already been performed (see `validation` and `domain` modules).
#[async_trait]
pub trait EntryStore {
    /// Associated type representing an `Entry` retrieved from storage.
    type Entry: AsEntry + AsEncodedEntry + Clone;

    /// Insert an `Entry` to the store in it's encoded and decoded form. Optionally also store it's encoded
    /// operation.
    ///
    /// `Entries` are decoded on arrival to a node and their decoded values would be persisted using this
    /// method. We also expect the encoded form to be persisted to avoid encoding values once again when
    /// replicating `Entries` to other peers.
    ///
    /// Returns an error if a fatal storage error occurred.
    async fn insert_entry(
        &self,
        entry: &P2pandaEntry,
        encoded_entry: &EncodedEntry,
        encoded_operation: Option<&EncodedOperation>,
    ) -> Result<(), EntryStorageError>;

    /// Get an `Entry` at sequence position within a `PublicKey`'s log.
    ///
    /// Returns a result containing an `Entry` or if no `Entry` could be found at this `PublicKey`,
    /// `LogId`, `SeqNum` location then `None` is returned. Errors when a fatal storage error occurs.
    async fn get_entry_at_seq_num(
        &self,
        public_key: &PublicKey,
        log_id: &LogId,
        seq_num: &SeqNum,
    ) -> Result<Option<Self::Entry>, EntryStorageError>;

    /// Get an `Entry` by it's `Hash`.
    ///
    /// Returns a result containing an `Entry` wrapped in an option. If no `Entry` could
    /// be found for this hash then None is returned. Errors when a fatal storage error
    /// occurs.
    async fn get_entry(&self, hash: &Hash) -> Result<Option<Self::Entry>, EntryStorageError>;

    /// Get the latest `Entry` of `PublicKey`'s log.
    ///
    /// Returns a result containing an `Entry` wrapped in an option. If no log was
    /// could be found at this `PublicKey` - log location then None is returned.
    /// Errors when a fatal storage error occurs.
    async fn get_latest_entry(
        &self,
        public_key: &PublicKey,
        log_id: &LogId,
    ) -> Result<Option<Self::Entry>, EntryStorageError>;
}
