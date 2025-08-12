// SPDX-License-Identifier: MIT OR Apache-2.0

//! Generic interfaces required by `p2panda-auth` data-types.
use std::fmt::Debug;
use std::hash::Hash as StdHash;

mod dgm;
mod operation;
mod orderer;
mod resolver;

pub use dgm::{GroupMembership, Groups};
pub use operation::Operation;
pub use orderer::Orderer;
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

/// Conditions associated with an actors access level.
pub trait Conditions: Clone + Debug + PartialEq + PartialOrd {}
