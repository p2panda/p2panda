// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;

use p2panda_core::traits::{Author, OperationId};

/// Interface for implementing a custom group CRDT resolver.
pub trait Resolver<ID, OP, M, C>
where
    ID: Author,
    OP: OperationId,
{
    type State;

    type Error: Debug;

    /// Check if this message requires that a full state re-build takes place. This would usually be
    /// due to concurrent operations arriving which require special handling.
    fn rebuild_required(y: &Self::State, msg: &M) -> Result<bool, Self::Error>;

    /// Process all operations and update internal state as required.
    ///
    /// This could include updating any internal filter object.
    fn process(y: Self::State) -> Result<Self::State, Self::Error>;
}
