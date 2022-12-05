// SPDX-License-Identifier: AGPL-3.0-or-later

//! Interfaces for interactions for operation-like structs.
use crate::document::DocumentViewId;
use crate::identity::PublicKey;
use crate::operation::plain::PlainFields;
use crate::operation::{OperationAction, OperationFields, OperationId, OperationVersion};
use crate::schema::SchemaId;

/// Trait representing a struct encapsulating data which has been signed by an author.
/// 
/// The method returns the public key of the keypair used to perform signing.
pub trait WithPublicKey {
    /// Returns the public key of the author of this entry or operation.
    fn public_key(&self) -> &PublicKey;
}

/// Trait representing the id of an "operation-like" struct.
/// 
/// Returns an operation's id which is derived from the hash of the entry it was published with. 
pub trait WithOperationID {
    /// Returns the identifier for this operation.
    fn id(&self) -> &OperationId;
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
    fn previous(&self) -> Option<DocumentViewId>;

    /// Returns true if operation contains fields.
    fn has_fields(&self) -> bool {
        self.fields().is_some()
    }

    /// Returns true if previous contains a document view id.
    fn has_previous_operations(&self) -> bool {
        self.previous().is_some()
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
/// Contains the values of an operation as well as it's id and the public key of it's author.
/// The reason an unpublished operation has no id is that the id is derived from the hash of
/// the signed entry an operation is encoded on.
///
/// [`StorageProvider`][crate::storage_provider::traits::StorageProvider] implementations should
/// implement this for a data structure that represents an operation as it is stored in the
/// database.
pub trait AsVerifiedOperation: AsOperation {
    /// Returns the identifier for this operation.
    fn id(&self) -> &OperationId;

    /// Returns the public key of the author of this operation.
    fn public_key(&self) -> &PublicKey;
}
