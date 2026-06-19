// SPDX-License-Identifier: MIT OR Apache-2.0

use std::borrow::Borrow;

use p2panda_core::traits::{Digest, Provenance};
use p2panda_core::{Hash, VerifyingKey};

use crate::message::SpacesArgs;

/// Interface for wrapping forge args in custom message types.
pub trait Forge<C> {
    type Message: Provenance<VerifyingKey> + Digest<Hash> + Borrow<SpacesArgs<C>>;

    type Error: std::error::Error;

    /// Public key of the local peer.
    fn verifying_key(&self) -> VerifyingKey;

    /// Forge and persist a new message.
    fn forge(
        &self,
        args: SpacesArgs<C>,
    ) -> impl Future<Output = Result<Self::Message, Self::Error>>;
}
