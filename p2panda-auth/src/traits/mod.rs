// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;
use std::hash::Hash as StdHash;

mod dgm;
mod group;
mod group_store;
mod operation;
mod ordering;
mod query;
mod resolver;

pub use dgm::GroupMembership;
pub use group::AuthGroup;
pub use group_store::GroupStore;
pub use operation::Operation;
pub use ordering::Ordering;
pub use query::GroupMembershipQuery;
pub use resolver::Resolver;

/// Handle to identify a group member.
///
/// Note that this needs to be unique within a group, can be a username, number or preferably a
/// long byte string.
pub trait IdentityHandle: Copy + Debug + PartialEq + Eq + StdHash {}

/// Identifier for each group membership operation.
///
/// Operations trigger changes of the group state and are usually sent in form of messages over the
/// network. Each operation needs to be uniquely identifiable, preferably by a collision-resistant
/// hash.
pub trait OperationId: Copy + Debug + PartialEq + Eq + StdHash {}
