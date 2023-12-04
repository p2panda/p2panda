// SPDX-License-Identifier: AGPL-3.0-or-later

//! Interfaces for interactions for operation-like structs.
use crate::identity_v2::PublicKey;
use crate::operation_v2::{OperationAction, OperationFields};

use super::body::traits::Schematic;
use super::header::traits::{Actionable, Authored};

/// @TODO: This can be removed in a later step as it duplicates the function of the 
/// `Authored` trait.
/// 
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
pub trait AsOperation: Actionable + Authored + Schematic {
    /// Returns application data fields of operation.
    fn fields(&self) -> Option<&OperationFields>;

    /// Returns true if operation contains fields.
    fn has_fields(&self) -> bool {
        self.fields().is_some()
    }

    /// Returns true if previous contains a document view id.
    fn has_previous_operations(&self) -> bool {
        self.previous().is_some()
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
