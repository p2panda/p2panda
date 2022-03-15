// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::{TryFrom, TryInto};
use std::fmt::Debug;

use crate::document::DocumentId;
use crate::entry::{decode_entry, Entry, EntrySigned, LogId};
use crate::identity::Author;
use crate::operation::OperationEncoded;
use crate::schema::SchemaId;
use crate::storage_provider::models::EntryWithOperation;

/// Trait to be implemented on a struct representing a stored entry.
///
/// Should be defined within a specific storage implementation on a data struct which
/// represents an entry as it is stored in the database. This trait defines methods for
/// reading values from the entry and it's operation and ensures the required conversion
/// (to and from `EntryWithOperation`) are present.
pub trait AsStorageEntry:
    Sized + Clone + Send + Sync + TryInto<EntryWithOperation> + TryFrom<EntryWithOperation>
{
    /// The error type returned by this traits' methods.
    type AsStorageEntryError: Debug;

    /// Return the encoded entry.
    fn entry_encoded(&self) -> EntrySigned;

    /// Returns the optional encoded operation.
    fn operation_encoded(&self) -> Option<OperationEncoded>;

    /// Returns the decoded operation.
    fn entry_decoded(&self) -> Entry {
        // Unwrapping as validation occurs in `EntryWithOperation`.
        decode_entry(&self.entry_encoded(), self.operation_encoded().as_ref()).unwrap()
    }
}

/// Trait to be implemented on a struct representing a stored log.
///
/// Should be defined within a specific storage implementation on a data struct which
/// represents a log as it is stored in the database. This trait defines methods for
/// reading values from the log.
///
/// NB: Currently there is no struct representing a log in p2panda-rs, if we choose to
/// bring something in, then we can also define conversion traits here.
pub trait AsStorageLog: Sized + Send + Sync {
    /// Constructor method for struts
    fn new(author: &Author, document: &DocumentId, schema: &SchemaId, log_id: &LogId) -> Self;

    /// Returns the Author of this log.
    fn author(&self) -> Author;

    /// Returns the LogId of this log.
    fn log_id(&self) -> LogId;

    /// Returns the DocumentId of this log.
    fn document(&self) -> DocumentId;

    /// Returns the SchemaId of this log.
    fn schema(&self) -> SchemaId;
}
