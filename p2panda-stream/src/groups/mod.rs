// SPDX-License-Identifier: MIT OR Apache-2.0

//! Types and methods for ordering and processing groups operations.
mod args;
mod operation;
mod processor;

pub use args::GroupsArgs;
pub use operation::GroupsOperation;
pub use processor::{Groups, GroupsError};
