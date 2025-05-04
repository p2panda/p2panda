use std::fmt::Debug;
use std::hash::Hash as StdHash;

mod auth_graph;
mod operation;
mod ordering;
mod resolver;

pub use auth_graph::AuthGraph;
pub use operation::Operation;
pub use ordering::Ordering;
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
