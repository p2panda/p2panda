// SPDX-License-Identifier: MIT OR Apache-2.0

//! Trait interfaces for managing key material and forging messages.
use std::fmt::Debug;

use p2panda_core::PublicKey;
use p2panda_encryption::key_manager::PreKeyBundlesState;
use p2panda_encryption::key_registry::KeyRegistryState;

use crate::ActorId;
use crate::message::SpacesArgs;

pub trait Forge<ID, M, C> {
    type Error: Debug;

    /// Public key of the local peer.
    fn public_key(&self) -> PublicKey;

    /// Forge and persist a new message.
    fn forge(&self, args: SpacesArgs<ID, C>) -> impl Future<Output = Result<M, Self::Error>>;
}

pub trait KeyRegistryStore {
    type Error: Debug;

    fn key_registry(&self) -> impl Future<Output = Result<KeyRegistryState<ActorId>, Self::Error>>;

    fn set_key_registry(
        &self,
        y: &KeyRegistryState<ActorId>,
    ) -> impl Future<Output = Result<(), Self::Error>>;
}

pub trait KeySecretStore {
    type Error: Debug;

    fn prekey_secrets(&self) -> impl Future<Output = Result<PreKeyBundlesState, Self::Error>>;

    fn set_prekey_secrets(
        &self,
        y: &PreKeyBundlesState,
    ) -> impl Future<Output = Result<(), Self::Error>>;
}
