// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;

use p2panda_core::PublicKey;

mod dedup;
pub mod log_sync;
pub mod manager;
pub mod map;
pub mod protocol;
#[cfg(any(test, feature = "test_utils"))]
pub mod test_utils;
pub mod topic_handshake;
pub mod traits;

pub use manager::{TopicSyncManager, TopicSyncManagerConfig};
pub use map::SessionTopicMap;
pub use protocol::{TopicLogSync, TopicLogSyncError, TopicLogSyncEvent, TopicLogSyncMessage};

/// A map of author logs.
pub type Logs<L> = HashMap<PublicKey, Vec<L>>;

/// Configuration object for instantiated sync sessions.
#[derive(Clone, Debug)]
pub struct SyncSessionConfig<T> {
    pub topic: T,
    pub remote: PublicKey,
    pub live_mode: bool,
}

/// Message sent to running sync sessions.
#[derive(Clone, Debug)]
pub enum ToSync<M> {
    Payload(M),
    Close,
}

/// Events which are emitted from a manager.
#[derive(Clone, PartialEq, Debug)]
pub struct FromSync<E> {
    pub session_id: u64,
    pub remote: PublicKey,
    pub event: E,
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
