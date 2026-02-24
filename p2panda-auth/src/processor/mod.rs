// SPDX-License-Identifier: MIT OR Apache-2.0

//! Types and methods for ordering and processing groups control messages.
mod args;
mod operation;
#[allow(clippy::module_inception)]
mod processor;
mod store;

pub use args::GroupsArgs;
pub use operation::GroupsOperation;
pub use processor::{GroupsProcessor, GroupsProcessorError};
pub use store::{AuthState, Store};
