// SPDX-License-Identifier: AGPL-3.0-or-later

//! Interfaces for interactions for operation-like structs.
use crate::document::{DocumentId, DocumentViewId};
use crate::hash::Hash;
use crate::identity::{PublicKey, Signature};
use crate::operation::{OperationAction, OperationFields, OperationId};

use super::OperationVersion;

/// Methods associated with identifying an operation and it's document.
pub trait Identifiable {
    /// Id of this operation.
    fn id(&self) -> &OperationId;

    /// Id of the document this operation applies to.
    fn document_id(&self) -> DocumentId;
}

/// Methods required for handling author capabilities.
pub trait Capable: Authored {
    /// Hash of the preceding operation in an authors log, None if this is the first operation.
    fn backlink(&self) -> Option<&Hash>;

    /// The distance (via the longest path) from this operation to the root of the operation graph.
    fn depth(&self) -> u64;
}

/// Methods available on signed data.
pub trait Authored {
    /// The public key of the keypair which signed this data.
    fn public_key(&self) -> &PublicKey;
    
    /// The signature.
    fn signature(&self) -> Signature;
}

/// Method available on data which has a timestamp.
pub trait Timestamped {
    /// Timestamp when this operation was published.
    fn timestamp(&self) -> u128;
}

/// Methods for retrieving metadata of an operations payload.
pub trait Payloaded {
    /// Size size in bytes of the payload.
    fn payload_size(&self) -> u64;

    /// Hash of the payload.
    fn payload_hash(&self) -> &Hash;
}

/// Methods available on an operation which contains OperationFields in it's payload.
pub trait Fielded {
    /// Returns application data fields of operation.
    fn fields(&self) -> Option<&OperationFields>;

    /// Returns true if operation contains fields.
    fn has_fields(&self) -> bool {
        self.fields().is_some()
    }
}

pub trait Actionable {
    /// Returns the operation version.
    fn version(&self) -> OperationVersion;

    /// Returns the operation action.
    fn action(&self) -> OperationAction;

    /// Returns a list of previous operations.
    fn previous(&self) -> Option<&DocumentViewId>;

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
