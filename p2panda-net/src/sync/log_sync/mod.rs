// SPDX-License-Identifier: MIT OR Apache-2.0

mod api;
mod builder;
#[cfg(test)]
mod tests;

pub use api::{LogSync, LogSyncError};
pub use builder::Builder;

const LOG_SYNC_PROTOCOL_ID: &[u8] = b"p2panda/log_sync/v1";
