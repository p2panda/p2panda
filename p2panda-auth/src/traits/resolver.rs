// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::group::GroupCrdtError;
use crate::group::{GroupControlMessage, GroupCrdtState};
use crate::traits::{GroupStore, IdentityHandle, OperationId, Orderer};

/// Interface for implementing a custom group crdt resolver.
pub trait Resolver<ID, OP, C, ORD, GS>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    ORD: Orderer<ID, OP, GroupControlMessage<ID, C>>,
    GS: GroupStore<ID, OP, C, Self, ORD>,
    Self: Sized,
{
    /// Check if this message requires that a full state re-build takes place. This would usually
    /// be due to concurrent operations arriving which require special handling.
    fn rebuild_required(
        y: &GroupCrdtState<ID, OP, C, Self, ORD, GS>,
        msg: &ORD::Operation,
    ) -> Result<bool, GroupCrdtError<ID, OP, C, Self, ORD, GS>>;

    /// Process all operations and update internal state as required.
    ///
    /// This could include updating any internal filter object.
    #[allow(clippy::type_complexity)]
    fn process(
        y: GroupCrdtState<ID, OP, C, Self, ORD, GS>,
    ) -> Result<GroupCrdtState<ID, OP, C, Self, ORD, GS>, GroupCrdtError<ID, OP, C, Self, ORD, GS>>;
}
