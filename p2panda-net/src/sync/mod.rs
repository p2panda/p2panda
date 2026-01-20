// SPDX-License-Identifier: MIT OR Apache-2.0

//! Eventually consistent, local-first sync protocols.
mod actors;
mod handle;
mod log_sync;
#[cfg(test)]
mod tests;

pub use handle::{SyncHandle, SyncHandleError, SyncSubscription};
pub use log_sync::{Builder, LogSync, LogSyncError};
