// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::document::{DocumentViewId, DocumentId};
use crate::hash::Hash;
use crate::identity::{PublicKey, Signature};
use crate::operation::{OperationVersion, OperationAction};

pub trait Authored {
    fn public_key(&self) -> &PublicKey;
    fn payload_size(&self) -> u64;
    fn payload_hash(&self) -> &Hash;
    fn signature(&self) -> Signature;
}

pub trait Actionable {
    /// Returns the operation version.
    fn version(&self) -> OperationVersion;

    /// Returns the operation action.
    fn action(&self) -> OperationAction;

    /// Returns a list of previous operations.
    fn previous(&self) -> Option<&DocumentViewId>;
}
