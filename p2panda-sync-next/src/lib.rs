// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::PublicKey;

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
    pub remote: PublicKey,
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
pub struct FromSync<E> {
    session_id: u64,
    remote: PublicKey,
    event: E,
}

impl<E> FromSync<E> {
    pub fn session_id(&self) -> u64 {
        self.session_id
    }

    pub fn event(&self) -> &E {
        &self.event
    }

    pub fn remote(&self) -> &PublicKey {
        &self.remote
    }
}
