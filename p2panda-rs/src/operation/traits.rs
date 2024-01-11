// SPDX-License-Identifier: AGPL-3.0-or-later

//! Interfaces for interactions for operation-like structs.
use crate::document::{DocumentId, DocumentViewId};
use crate::hash::Hash;
use crate::identity::{PublicKey, Signature};
use crate::operation::{OperationAction, OperationFields, OperationId};

use super::header::SeqNum;
use super::OperationVersion;

/// Methods associated with identifying an operation and it's document.
pub trait Identifiable {
    /// Id of this operation.
    fn id(&self) -> &OperationId;

    /// Id of the document this operation applies to.
    fn document_id(&self) -> DocumentId;
}

/// Properties required when wanting to verify the authenticity and cryptographic soundness of an operation.
pub trait Verifiable {
    /// The signature.
    fn signature(&self) -> Signature;

    /// Size size in bytes of the payload.
    fn payload_size(&self) -> u64;

    /// Hash of the payload.
    fn payload_hash(&self) -> &Hash;

    /// Hash of the preceding operation in an authors log, None if this is the first operation.
    fn backlink(&self) -> Option<&Hash>;
}

/// Method returning the public key of a signed piece of data.
pub trait Authored {
    /// The public key of the keypair which signed this data.
    fn public_key(&self) -> &PublicKey;
}

/// Method available on data which has a sequence number.
pub trait Sequenced {
    /// Sequence number of this operation.
    fn seq_num(&self) -> SeqNum;
}

/// Method available on data which has a timestamp.
pub trait Timestamped {
    /// Timestamp when this operation was published.
    fn timestamp(&self) -> u64;
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
