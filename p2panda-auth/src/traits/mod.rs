// SPDX-License-Identifier: MIT OR Apache-2.0

//! Generic interfaces required by `p2panda-auth` data-types.
use std::fmt::Debug;

mod dgm;
mod operation;
mod resolver;

pub use dgm::{GroupMembership, Groups};
pub use operation::Operation;
pub use resolver::Resolver;

/// Conditions associated with an actors access level.
pub trait Conditions: Clone + Debug + PartialEq + PartialOrd {}
