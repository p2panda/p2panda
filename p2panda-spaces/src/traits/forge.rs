// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;

use p2panda_core::PublicKey;

use crate::message::SpacesArgs;

/// Interface for wrapping forge args in custom message types.
pub trait Forge<ID, M, C> {
    type Error: Debug;

    /// Public key of the local peer.
    fn public_key(&self) -> PublicKey;

    /// Forge and persist a new message.
    fn forge(&self, args: SpacesArgs<ID, C>) -> impl Future<Output = Result<M, Self::Error>>;
}
