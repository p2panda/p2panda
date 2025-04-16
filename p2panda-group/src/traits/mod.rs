// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;
use std::hash::Hash as StdHash;

mod dgm;
mod key_bundle;
mod key_manager;
mod key_registry;

pub use dgm::AckedGroupMembership;
pub use key_bundle::KeyBundle;
pub use key_manager::{IdentityManager, PreKeyManager};
pub use key_registry::{IdentityRegistry, PreKeyRegistry};

/// Handle to identify a group member.
///
/// Note that this needs to be unique within a group, can be a username, number or preferably a
/// long byte string.
pub trait IdentityHandle: Copy + Debug + PartialEq + Eq + StdHash {}

#[cfg(any(test, feature = "test_utils"))]
impl IdentityHandle for &str {}

#[cfg(any(test, feature = "test_utils"))]
impl IdentityHandle for usize {}

/// Identifier for each group membership operation.
///
/// Operations trigger changes of the group state and are usually sent in form of messages over the
/// network. Each operation needs to be uniquely identifiable, preferably by a collision-resistant
/// hash.
pub trait OperationId: Copy + Debug + PartialEq + Eq + StdHash {}

#[cfg(any(test, feature = "test_utils"))]
impl OperationId for (usize, usize) {} // (ID, Seq)

#[cfg(any(test, feature = "test_utils"))]
impl OperationId for usize {}
