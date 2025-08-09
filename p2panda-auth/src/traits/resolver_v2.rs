// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::traits::{IdentityHandle, OperationId};

/// Interface for implementing a custom group crdt resolver.
pub trait Resolver<ID, OP, C, M>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
{
    type State;
    type Error;

    /// Check if this message requires that a full state re-build takes place. This would usually
    /// be due to concurrent operations arriving which require special handling.
    fn rebuild_required(y: &Self::State, msg: &M) -> Result<bool, Self::Error>;

    /// Process all operations and update internal state as required.
    ///
    /// This could include updating any internal filter object.
    #[allow(clippy::type_complexity)]
    fn process(y: Self::State) -> Result<Self::State, Self::Error>;
}
