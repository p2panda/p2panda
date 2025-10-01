// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;

use p2panda_core::PublicKey;
use p2panda_encryption::key_manager::KeyManagerState;
use p2panda_encryption::key_registry::KeyRegistryState;

use crate::ActorId;
use crate::message::SpacesArgs;

pub trait Forge<ID, M, C> {
    type Error: Debug;

    fn public_key(&self) -> PublicKey;

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

pub trait KeyManagerStore {
    type Error: Debug;

    fn key_manager(&self) -> impl Future<Output = Result<KeyManagerState, Self::Error>>;

    fn set_key_manager(&self, y: &KeyManagerState)
    -> impl Future<Output = Result<(), Self::Error>>;
}
