// SPDX-License-Identifier: AGPL-3.0-or-later

use std::hash::Hash as StdHash;

use crate::identity::Author;
use crate::next::entry::Entry;
use crate::next::operation::traits::AsVerifiedOperation;
use crate::next::operation::{Operation, OperationId};

/// An operation which has been encoded and published on a signed entry.
///
/// Contains the values of an operation as well as its author and id. This
/// [operation id][OperationId] is only available on [`VerifiedOperation`] and not on
/// [`Operation`] because it is derived from the hash of the signed entry an operation is encoded
/// on.
// @TODO: Fix pub(crate) visibility
#[derive(Debug, Clone, PartialEq)]
pub struct VerifiedOperation {
    /// Identifier of the operation.
    pub(crate) operation_id: OperationId,

    /// Operation, which is the payload of the entry.
    pub(crate) operation: Operation,

    /// Entry which was used to publish this operation.
    pub(crate) entry: Entry,
}

impl VerifiedOperation {
    /// Returns the entry related to this operation.
    pub fn entry(&self) -> &Entry {
        &self.entry
    }
}

impl AsVerifiedOperation for VerifiedOperation {
    /// Returns the identifier for this operation.
    fn operation_id(&self) -> &OperationId {
        &self.operation_id
    }

    /// Returns the wrapped operation.
    fn operation(&self) -> &Operation {
        &self.operation
    }

    /// Returns the public key of the author of this operation.
    fn public_key(&self) -> &Author {
        &self.entry.public_key()
    }
}

impl StdHash for VerifiedOperation {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.operation_id().hash(state)
    }
}

#[cfg(test)]
impl VerifiedOperation {
    /// Create a verified operation from it's unverified parts for testing.
    pub fn new(entry: &Entry, operation: &Operation, operation_id: &OperationId) -> Self {
        Self {
            operation_id: operation_id.to_owned(),
            operation: operation.to_owned(),
            entry: entry.to_owned(),
        }
    }
}
