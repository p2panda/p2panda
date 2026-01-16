// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::PublicKey;

mod dedup;
pub mod manager;
pub mod protocols;
#[cfg(any(test, feature = "test_utils"))]
pub mod test_utils;
pub mod traits;

/// Configuration object for instantiating sync sessions.
#[derive(Clone, Debug)]
pub struct SessionConfig<T> {
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
