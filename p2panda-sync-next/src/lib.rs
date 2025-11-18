// SPDX-License-Identifier: MIT OR Apache-2.0

pub mod dedup;
pub mod managers;
pub mod protocols;
pub mod session_topic_map;
#[cfg(any(test, feature = "test_utils"))]
pub mod test_utils;
pub mod traits;

pub use dedup::{DEFAULT_BUFFER_CAPACITY, Dedup};
pub use managers::topic_sync_manager::TopicSyncManager;
pub use protocols::{log_sync, topic_handshake, topic_log_sync};
pub use session_topic_map::SessionTopicMap;

/// Configuration object for instantiated sync sessions.
#[derive(Clone, Debug)]
pub struct SyncSessionConfig<T> {
    pub topic: T,
    pub live_mode: bool,
}

/// Message sent to running sync sessions.
#[derive(Clone, Debug)]
pub enum ToSync {
    Payload(Vec<u8>),
    Close,
}

/// Events which are emitted from a manager.
#[derive(Clone, Debug)]
pub enum SyncManagerEvent<E> {
    /// Generic events emitted from a sync protocol implementation.
    FromSync { session_id: u64, event: E },
}

impl<E> SyncManagerEvent<E> {
    pub fn session_id(&self) -> u64 {
        match self {
            SyncManagerEvent::FromSync { session_id, .. } => *session_id,
        }
    }
}
