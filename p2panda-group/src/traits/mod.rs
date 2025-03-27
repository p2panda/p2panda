// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;
use std::hash::Hash as StdHash;

mod key_bundle;
mod key_manager;
mod key_registry;

pub use key_bundle::KeyBundle;
pub use key_manager::{IdentityManager, PreKeyManager};
pub use key_registry::{IdentityRegistry, PreKeyRegistry};

pub trait IdentityHandle: Copy + Debug + PartialEq + Eq + StdHash {}

#[cfg(test)]
impl IdentityHandle for &str {}

pub trait OperationId: Copy + Debug + PartialEq + Eq + StdHash {}

#[cfg(test)]
impl OperationId for usize {}
