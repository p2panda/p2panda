// SPDX-License-Identifier: MIT OR Apache-2.0

mod manager;
mod poller;
mod session;
mod stream;
#[cfg(test)]
mod tests;

pub use manager::{SyncManager, ToSyncManager};
pub use stream::{SyncStream, ToSyncStream};

pub const SYNC_PROTOCOL_ID: &[u8] = b"p2panda/log_sync/v1";
