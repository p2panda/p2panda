// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;

use p2panda_auth::traits::Conditions;
use p2panda_encryption::key_manager::PreKeyBundlesState;
use p2panda_encryption::key_registry::KeyRegistryState;
use p2panda_spaces::space::SpaceState;
use p2panda_spaces::traits::SpaceId;
use p2panda_spaces::{ActorId, AuthGroupState};

/// Interface for setting and getting auth state.
pub trait AuthStore<C>
where
    C: Conditions,
{
    type Error: Debug;

    fn auth(&self) -> impl Future<Output = Result<AuthGroupState<C>, Self::Error>>;

    fn set_auth(&self, y: &AuthGroupState<C>) -> impl Future<Output = Result<(), Self::Error>>;
}

/// Interface for setting and getting key registry state and pre-key secrets.
pub trait EncryptionKeyStore {
    type Error: Debug;

    fn key_registry(&self) -> impl Future<Output = Result<KeyRegistryState<ActorId>, Self::Error>>;

    fn set_key_registry(
        &self,
        y: &KeyRegistryState<ActorId>,
    ) -> impl Future<Output = Result<(), Self::Error>>;

    fn prekey_secrets(&self) -> impl Future<Output = Result<PreKeyBundlesState, Self::Error>>;

    fn set_prekey_secrets(
        &self,
        y: &PreKeyBundlesState,
    ) -> impl Future<Output = Result<(), Self::Error>>;
}

/// Interface for setting and getting space state.
pub trait SpacesStore<ID, M, C>
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

    fn spaces_ids(&self) -> impl Future<Output = Result<Vec<ID>, Self::Error>>;

    fn set_space(
        &self,
        id: &ID,
        y: SpaceState<ID, M, C>,
    ) -> impl Future<Output = Result<(), Self::Error>>;
}
