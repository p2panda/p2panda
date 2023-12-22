// SPDX-License-Identifier: AGPL-3.0-or-later

//! Interfaces for interactions for operation-like structs.
use crate::document::DocumentId;
use crate::hash::Hash;
use crate::operation::body::traits::Schematic;
use crate::operation::header::traits::{Actionable, Authored};
use crate::operation::{OperationAction, OperationFields, OperationId};

/// Trait representing an "operation-like" struct.
///
/// Structs which "behave like" operations have a version and a distinct action. They can also
/// relate to previous operations to form an operation graph.
pub trait AsOperation: Actionable + Authored + Schematic {
    /// Id of this operation.
    fn id(&self) -> &OperationId;

    /// Id of the document this operation applies to.
    fn document_id(&self) -> DocumentId;
    
    /// Timestamp when this operation was published.
    fn timestamp(&self) -> u128;

    /// Hash of the preceding operation in an authors log, None if this is the first operation.
    fn backlink(&self) -> Option<&Hash>;

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
