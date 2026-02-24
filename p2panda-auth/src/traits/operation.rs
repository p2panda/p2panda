// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::group::GroupAction;

/// Interface to express required information from operations processed by any auth graph
/// implementation.
///
/// Applications implementing these traits should authenticate the original sender of each
/// operation.
pub trait Operation<ID, OP, C = ()> {
    /// Id of this operation.
    fn id(&self) -> OP;

    /// ID of the author of this operation.
    fn author(&self) -> ID;

    /// Auth dependencies.
    fn dependencies(&self) -> Vec<OP>;

    /// The id of the group the action applies to.
    fn group_id(&self) -> ID;

    /// The group action.
    fn action(&self) -> GroupAction<ID, C>;
}
