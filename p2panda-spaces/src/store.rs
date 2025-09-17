// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;

use p2panda_auth::traits::Conditions;
use p2panda_encryption::key_manager::KeyManagerState;
use p2panda_encryption::key_registry::KeyRegistryState;

use crate::OperationId;
use crate::space::SpaceState;
use crate::traits::SpaceId;
use crate::types::{ActorId, AuthGroupState};

pub trait SpaceStore<ID, M, C>
where
    ID: SpaceId,
    C: Conditions,
{
    type Error: Debug;

    fn space(
        &self,
        id: &ID,
    ) -> impl Future<Output = Result<Option<SpaceState<ID, M, C>>, Self::Error>>;

    fn has_space(&self, id: &ID) -> impl Future<Output = Result<bool, Self::Error>>;

    fn set_space(
        &mut self,
        id: &ID,
        y: SpaceState<ID, M, C>,
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

pub trait AuthStore<C>
where
    C: Conditions,
{
    type Error: Debug;

    fn auth(&self) -> impl Future<Output = Result<AuthGroupState<C>, Self::Error>>;

    fn set_auth(&mut self, y: &AuthGroupState<C>) -> impl Future<Output = Result<(), Self::Error>>;
}

pub trait MessageStore<M> {
    type Error: Debug;

    fn message(&self, id: &OperationId) -> impl Future<Output = Result<Option<M>, Self::Error>>;

    fn set_message(
        &mut self,
        id: &OperationId,
        message: &M,
    ) -> impl Future<Output = Result<(), Self::Error>>;
}
