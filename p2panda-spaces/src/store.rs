// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;

use p2panda_auth::traits::Conditions;
use p2panda_encryption::key_manager::KeyManagerState;
use p2panda_encryption::key_registry::KeyRegistryState;

use crate::space::SpaceState;
use crate::types::ActorId;

pub trait SpaceStore<M, C>
where
    C: Conditions,
{
    type Error: Debug;

    fn space(
        &self,
        id: &ActorId,
    ) -> impl Future<Output = Result<Option<SpaceState<M, C>>, Self::Error>>;

    fn has_space(&self, id: &ActorId) -> impl Future<Output = Result<bool, Self::Error>>;

    fn set_space(
        &mut self,
        id: &ActorId,
        y: SpaceState<M, C>,
    ) -> impl Future<Output = Result<(), Self::Error>>;
}

pub trait KeyStore {
    type Error: Debug;

    fn key_manager(&self) -> impl Future<Output = Result<KeyManagerState, Self::Error>>;

    fn key_registry(&self) -> impl Future<Output = Result<KeyRegistryState<ActorId>, Self::Error>>;

    fn set_key_manager(
        &mut self,
        y: &KeyManagerState,
    ) -> impl Future<Output = Result<(), Self::Error>>;

    fn set_key_registry(
        &mut self,
        y: &KeyRegistryState<ActorId>,
    ) -> impl Future<Output = Result<(), Self::Error>>;
}
