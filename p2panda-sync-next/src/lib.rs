// SPDX-License-Identifier: MIT OR Apache-2.0

#[cfg(any(test, feature = "cbor"))]
pub mod cbor;
pub mod dedup;
pub mod manager;
pub mod protocols;
pub mod session_topic_map;
#[cfg(any(test, feature = "test_utils"))]
pub mod test_utils;
pub mod traits;

pub use dedup::{DEFAULT_BUFFER_CAPACITY, Dedup};
pub use protocols::{log_sync, topic_handshake, topic_log_sync};
pub use session_topic_map::SessionTopicMap;

/// Configuration object for instantiated sync sessions.
#[derive(Clone, Debug)]
pub struct SyncSessionConfig<T> {
    pub topic: Option<T>,
    pub live_mode: bool,
}

impl<T> Default for SyncSessionConfig<T> {
    fn default() -> Self {
        Self {
            topic: Default::default(),
            live_mode: true,
        }
    }
}

/// Message sent to running sync sessions.
#[derive(Clone, Debug)]
pub enum ToSync {
    Payload(Vec<u8>),
    Close,
}

/// Events which are emitted from a manager.
#[derive(Clone, Debug)]
pub enum SyncManagerEvent<T, E> {
    /// Emitted once both parties in the sync protocol have agreed on the topic for a session.
    ///
    /// Normally the initiator will suggest a topic, if the remote rejects this for any reason,
    /// this event will not be emitted. This event is emitted on both the initiator and receiver
    /// sides.
    TopicAgreed { session_id: u64, topic: T },

    /// Generic events emitted from a sync protocol implementation.
    FromSync { session_id: u64, event: E },
}

impl<T, E> SyncManagerEvent<T, E> {
    pub fn session_id(&self) -> u64 {
        match self {
            SyncManagerEvent::TopicAgreed { session_id, .. } => *session_id,
            SyncManagerEvent::FromSync { session_id, .. } => *session_id,
        }
    }
}
