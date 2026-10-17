// SPDX-License-Identifier: MIT OR Apache-2.0

//! Eventually consistent, local-first sync protocols.
mod actors;
pub mod authoriser;
mod handle;
pub mod hooks;
mod log_sync;
#[cfg(test)]
mod tests;

pub use handle::{SyncHandle, SyncHandleError, SyncSubscription};
pub use log_sync::{Builder, LogSync, LogSyncError, LogSyncRejected};
