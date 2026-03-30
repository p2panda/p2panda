// SPDX-License-Identifier: MIT OR Apache-2.0

//! Types and methods for ordering and processing groups operations.
mod args;
mod operation;
#[allow(clippy::module_inception)]
#[cfg(feature = "processor")]
mod processor;

pub use args::GroupsArgs;
pub use operation::GroupsOperation;
pub use processor::{GroupsProcessor, GroupsProcessorError};
