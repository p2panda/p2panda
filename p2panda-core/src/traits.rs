// SPDX-License-Identifier: MIT OR Apache-2.0

use std::hash::Hash as StdHash;

/// Identifier of a single operation.
pub trait OperationId: Copy + Clone + PartialEq + Eq + StdHash {}

/// Type returning it's own identifier.
pub trait Identifier<ID>
where
    ID: OperationId,
{
    fn id(&self) -> &ID;
}
