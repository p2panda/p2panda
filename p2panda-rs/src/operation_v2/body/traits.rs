// SPDX-License-Identifier: AGPL-3.0-or-later

//! Interfaces for interactions for operation-like structs.
use crate::document::DocumentViewId;
use crate::identity::PublicKey;
use crate::operation::plain::PlainFields;
use crate::operation::{OperationAction, OperationFields, OperationVersion};
use crate::schema::SchemaId;

/// Trait representing a struct encapsulating data which has been signed by an author.
///
/// The method returns the public key of the keypair used to perform signing.
pub trait WithPublicKey {
    /// Returns the public key of the author of this entry or operation.
    fn public_key(&self) -> &PublicKey;
}

/// Trait representing an "operation-like" struct.
///
/// Structs which "behave like" operations have a version and a distinct action. They can also
/// relate to previous operations to form an operation graph.
pub trait Actionable {
    /// Returns the operation version.
    fn version(&self) -> OperationVersion;

    /// Returns the operation action.
    fn action(&self) -> OperationAction;

    /// Returns a list of previous operations.
    fn previous(&self) -> Option<&DocumentViewId>;
}

/// Trait representing an "operation-like" struct which contains data fields that can be checked
/// against a schema.
pub trait Schematic {
    /// Returns the schema id.
    fn schema_id(&self) -> &SchemaId;

    /// Returns the fields holding the data.
    fn fields(&self) -> Option<PlainFields>;
}

/// Trait to be implemented on "operation-like" structs.
pub trait AsBody {
    /// Returns action type of operation.
    fn action(&self) -> OperationAction;

    /// Returns schema id of operation.
    fn schema_id(&self) -> SchemaId;

    /// Returns version of operation.
    fn version(&self) -> OperationVersion;

    /// Returns application data fields of operation.
    fn fields(&self) -> Option<OperationFields>;

    /// Returns true if operation contains fields.
    fn has_fields(&self) -> bool {
        self.fields().is_some()
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
