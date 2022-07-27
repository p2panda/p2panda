// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::identity::Author;
use crate::next::document::DocumentViewId;
use crate::next::operation::plain::PlainFields;
use crate::next::operation::{
    Operation, OperationAction, OperationFields, OperationId, OperationVersion,
};
use crate::next::schema::SchemaId;

pub trait Actionable {
    fn version(&self) -> OperationVersion;
    fn action(&self) -> OperationAction;
    fn previous_operations(&self) -> Option<&DocumentViewId>;
}

pub trait Schematic {
    fn schema_id(&self) -> &SchemaId;
    fn fields(&self) -> Option<PlainFields>;
}

/// Trait to be implemented on [`Operation`] and
/// [`VerifiedOperation`][crate::operation::VerifiedOperation] structs.
pub trait AsOperation {
    /// Returns action type of operation.
    fn action(&self) -> OperationAction;

    /// Returns schema id of operation.
    fn schema_id(&self) -> SchemaId;

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

/// Trait to be implemented on a struct representing an operation which has been encoded and
/// published on a signed entry.
///
/// Contains the values of an operation as well as it's author and id. The reason an unpublished
/// operation has no id is that the id is derived from the hash of the signed entry an operation is
/// encoded on.
///
/// [`StorageProvider`][crate::storage_provider::traits::StorageProvider] implementations should
/// implement this for a data structure that represents an operation as it is stored in the
/// database.
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
        AsOperation::action(AsVerifiedOperation::operation(self))
    }

    /// Returns schema if of operation.
    fn schema_id(&self) -> SchemaId {
        AsOperation::schema_id(AsVerifiedOperation::operation(self))
    }

    /// Returns version of operation.
    fn version(&self) -> OperationVersion {
        AsOperation::version(AsVerifiedOperation::operation(self))
    }

    /// Returns application data fields of operation.
    fn fields(&self) -> Option<OperationFields> {
        AsOperation::fields(AsVerifiedOperation::operation(self))
    }

    /// Returns vector of this operation's previous operation ids
    fn previous_operations(&self) -> Option<DocumentViewId> {
        AsOperation::previous_operations(AsVerifiedOperation::operation(self))
    }

    fn has_fields(&self) -> bool {
        self.fields().is_some()
    }

    fn has_previous_operations(&self) -> bool {
        self.previous_operations().is_some()
    }

    fn is_create(&self) -> bool {
        self.action() == OperationAction::Create
    }

    fn is_update(&self) -> bool {
        self.action() == OperationAction::Update
    }

    fn is_delete(&self) -> bool {
        self.action() == OperationAction::Delete
    }
}
