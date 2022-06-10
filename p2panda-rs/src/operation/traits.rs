// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::document::DocumentViewId;
use crate::identity::Author;
use crate::operation::{
    Operation, OperationAction, OperationFields, OperationId, OperationVersion,
};
use crate::schema::SchemaId;

pub trait AsOperation {
    /// Returns action type of operation.
    fn action(&self) -> OperationAction;

    /// Returns schema of operation.
    fn schema(&self) -> SchemaId;

    /// Returns version of operation.
    fn version(&self) -> OperationVersion;

    /// Returns application data fields of operation.
    fn fields(&self) -> Option<OperationFields>;

    /// Returns vector of this operation's previous operation ids
    fn previous_operations(&self) -> Option<DocumentViewId>;

    /// Returns true if operation contains fields.
    fn has_fields(&self) -> bool {
        self.fields().is_some()
    }

    /// Returns true if previous_operations contains a document view id.
    fn has_previous_operations(&self) -> bool {
        self.previous_operations().is_some()
    }

    /// Returns true when instance is CREATE operation.
    fn is_create(&self) -> bool {
        self.action() == OperationAction::Create
    }

    /// Returns true when instance is UPDATE operation.
    fn is_update(&self) -> bool {
        self.action() == OperationAction::Update
    }

    /// Returns true when instance is DELETE operation.
    fn is_delete(&self) -> bool {
        self.action() == OperationAction::Delete
    }
}

pub trait AsVerifiedOperation {
    /// Returns the identifier for this operation.
    fn operation_id(&self) -> &OperationId;

    /// Returns the public key of the author of this operation.
    fn public_key(&self) -> &Author;

    /// Returns the wrapped operation.
    fn operation(&self) -> &Operation;
}

impl<T: AsVerifiedOperation> AsOperation for T {
    /// Returns action type of operation.
    fn action(&self) -> OperationAction {
        self.operation().action()
    }

    /// Returns schema of operation.
    fn schema(&self) -> SchemaId {
        self.operation().schema()
    }

    /// Returns version of operation.
    fn version(&self) -> OperationVersion {
        self.operation().version()
    }

    /// Returns application data fields of operation.
    fn fields(&self) -> Option<OperationFields> {
        self.operation().fields()
    }

    /// Returns vector of this operation's previous operation ids
    fn previous_operations(&self) -> Option<DocumentViewId> {
        self.operation().previous_operations()
    }
}
