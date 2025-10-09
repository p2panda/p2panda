// SPDX-License-Identifier: MIT OR Apache-2.0

use std::hash::Hash as StdHash;

/// Identifier of a single operation.
pub trait OperationId: Copy + Clone + PartialEq + Eq + StdHash {}

/// Returns (unique) hash digest, which can be used as identifier of this published data type.
pub trait Digest<ID>
where
    ID: OperationId,
{
    fn hash(&self) -> ID;
}
