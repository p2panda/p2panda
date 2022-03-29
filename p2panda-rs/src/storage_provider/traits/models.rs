// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryInto;
use std::fmt::Debug;

use crate::document::DocumentId;
use crate::entry::{Entry, EntrySigned, LogId};
use crate::identity::Author;
use crate::operation::OperationEncoded;
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

    /// Return the encoded entry.
    fn entry_signed(&self) -> EntrySigned;

    /// Returns the optional encoded operation.
    fn operation_encoded(&self) -> Option<OperationEncoded>;

    /// Returns the decoded operation.
    fn entry_decoded(&self) -> Entry;
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
