// SPDX-License-Identifier: MIT OR Apache-2.0

//! Traits describing interfaces required by generic parameters or for integration of p2panda-auth
//! into other systems.
use std::fmt::Debug;
use std::hash::Hash as StdHash;

mod dgm;
mod operation;
mod orderer;
mod resolver;
mod store;

pub use dgm::{Group, GroupMembership};
pub use operation::Operation;
pub use orderer::Orderer;
pub use resolver::Resolver;
pub use store::GroupStore;

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
