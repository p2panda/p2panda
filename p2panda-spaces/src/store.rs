// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;

use crate::encryption::key_manager::KeyManagerState;
use crate::encryption::key_registry::KeyRegistryState;
use crate::space::SpaceState;
use crate::types::{ActorId, Conditions};

pub trait SpaceStore<M, C, RS>
where
    C: Conditions,
{
    type Error: Debug;

    fn space(&self, id: ActorId)
    -> impl Future<Output = Result<SpaceState<M, C, RS>, Self::Error>>;

    fn set_space(
        &self,
        id: ActorId,
        y: SpaceState<M, C, RS>,
    ) -> impl Future<Output = Result<SpaceState<M, C, RS>, Self::Error>>;
}

pub trait KeyStore {
    type Error: Debug;

    fn key_manager(&self) -> impl Future<Output = Result<KeyManagerState, Self::Error>>;

    fn key_registry(&self) -> impl Future<Output = Result<KeyRegistryState, Self::Error>>;

    fn set_key_manager(
        &mut self,
        y: &KeyManagerState,
    ) -> impl Future<Output = Result<(), Self::Error>>;

    fn set_key_registry(
        &mut self,
        y: &KeyRegistryState,
    ) -> impl Future<Output = Result<(), Self::Error>>;
}
