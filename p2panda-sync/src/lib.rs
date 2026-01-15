// SPDX-License-Identifier: MIT OR Apache-2.0

//! Data-type agnostic interfaces for implementing sync protocols and managers which can be used
//! stand-alone or as part of the local-first stack provided by
//! [`p2panda-net`].
//!
//! Users can implement two-party sync protocols over a `Sink` / `Stream` pair with the `Protocol`
//! trait and a system for instantiating and orchestrating concurrent sync sessions with the
//! `Manager` trait.
//!
//! Concrete implementations for performing sync over p2panda append-only logs associated with a
//! generic topic can be found in the `manager` and `protocols` modules.
//!
//! For most high-level users [`p2panda-net`]
//! will be the entry point into local-first development with p2panda. Interfaces in this crate
//! are intended for cases where users want to integrate their own base convergent data-type and
//! sync protocols as a module in the
//! [`p2panda-net`] stack.
//!
//! [`p2panda-net`]: https://docs.rs/p2panda-net/latest/p2panda_net/
use p2panda_core::PublicKey;

mod dedup;
pub mod manager;
pub mod protocols;
#[doc(hidden)]
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
