// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::hash::Hash;
use crate::identity::Author;
use crate::operation::{
    AsOperation, Operation, OperationAction, OperationFields, OperationVersion,
};

/// Wrapper struct containing an `Operation`, the hash of its entry, and the public key of its author.
#[derive(Debug, Clone, PartialEq)]
pub struct OperationWithMeta {
    operation_id: Hash,
    public_key: Author,
    operation: Operation,
}

impl OperationWithMeta {
    /// Create a new `OperationWithMeta`.
    pub fn new(id: &Hash, public_key: &Author, operation: &Operation) -> Self {
        Self {
            operation_id: id.to_owned(),
            public_key: public_key.to_owned(),
            operation: operation.to_owned(),
        }
    }

    /// Returns the operation_id for this operation.
    pub fn operation_id(&self) -> &Hash {
        &self.operation_id
    }

    /// Returns the public key of the author of this operation.
    pub fn public_key(&self) -> &Author {
        &self.public_key
    }

    /// Returns this operation.
    pub fn operation(&self) -> &Operation {
        &self.operation
    }
}

impl AsOperation for OperationWithMeta {
    /// Returns action type of operation.
    fn action(&self) -> &OperationAction {
        self.operation.action()
    }

    /// Returns version of operation.
    fn version(&self) -> &OperationVersion {
        self.operation.version()
    }

    /// Returns schema of operation.
    fn schema(&self) -> &Hash {
        self.operation.schema()
    }

    /// Returns id of the document this operation is part of.
    fn id(&self) -> Option<&Hash> {
        self.operation.id()
    }

    /// Returns user data fields of operation.
    fn fields(&self) -> Option<&OperationFields> {
        self.operation.fields()
    }
}
