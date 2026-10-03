// SPDX-License-Identifier: MIT OR Apache-2.0

//! Types and methods for ordering and processing groups operations.
mod args;
mod processor;

pub use args::GroupsProcessorArgs;
pub use processor::{Groups, GroupsError, GroupsResult};
