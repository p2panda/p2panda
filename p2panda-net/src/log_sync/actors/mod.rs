// SPDX-License-Identifier: MIT OR Apache-2.0

mod manager;
mod poller;
mod session;
#[cfg(test)]
mod tests;
mod topic_manager;

pub use manager::{SyncManager, ToSyncManager};
pub use topic_manager::{ToTopicManager, TopicManager};

pub const SYNC_PROTOCOL_ID: &[u8] = b"p2panda/log_sync/v1";
