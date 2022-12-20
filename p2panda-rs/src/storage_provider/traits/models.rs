// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::entry::traits::{AsEncodedEntry, AsEntry};
use crate::operation::EncodedOperation;

/// Trait to be implemented on a struct representing a stored entry optionally with it's payload.
///
/// Storage implementations should implement this for a data structure that represents an
/// entry as it is stored in the database. This trait requires implementations of both `AsEntry`
/// and `AsEncodedEntry` and additionally adds a method for accessing the entries'  payload.
pub trait EntryWithOperation: AsEntry + AsEncodedEntry {
    /// The payload of this operation.
    fn payload(&self) -> Option<&EncodedOperation>;
}
