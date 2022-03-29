// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryInto;
use std::fmt::Debug;

use crate::document::DocumentId;
use crate::entry::{LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::Author;
use crate::operation::Operation;
use crate::schema::SchemaId;
use crate::storage_provider::models::{EntryWithOperation, Log};

/// Trait to be implemented on a struct representing a stored entry.
///
/// Should be defined within a specific storage implementation on a data struct which represents an
/// entry as it is stored in the database. This trait defines methods for reading values from the
/// entry and it's operation and ensures the required conversion (to and from `EntryWithOperation`)
/// are present.
pub trait AsStorageEntry:
    Sized + Clone + Send + Sync + TryInto<EntryWithOperation> + From<EntryWithOperation>
{
    /// The error type returned by this traits' methods.
    type AsStorageEntryError: Debug;

    /// Returns the author of this entry.
    fn author(&self) -> Author;

    /// Returns the hash of this entry.
    fn hash(&self) -> Hash;

    /// Returns the bytes of the signed encoded entry.
    fn entry_bytes(&self) -> Vec<u8>;

    /// Returns hash of backlink entry when given.
    fn backlink_hash(&self) -> Option<Hash>;

    /// Returns hash of skiplink entry when given.
    fn skiplink_hash(&self) -> Option<Hash>;

    /// Returns the sequence number of this entry.
    fn seq_num(&self) -> SeqNum;

    /// Returns the log id of this entry.
    fn log_id(&self) -> LogId;

    /// Returns the operation contained on this entry.
    fn operation(&self) -> Operation;
}

/// Trait to be implemented on a struct representing a stored log.
///
/// Should be defined within a specific storage implementation on a data struct which represents a
/// log as it is stored in the database. This trait defines methods for reading values from the
/// log.
pub trait AsStorageLog: Sized + Send + Sync + TryInto<Log> + From<Log> {
    /// Constructor method for structs.
    fn new(log: Log) -> Self;

    /// Returns the Author of this log.
    fn author(&self) -> Author;

    /// Returns the LogId of this log.
    fn log_id(&self) -> LogId;

    /// Returns the DocumentId of this log.
    fn document(&self) -> DocumentId;

    /// Returns the SchemaId of this log.
    fn schema(&self) -> SchemaId;
}
