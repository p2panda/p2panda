// SPDX-License-Identifier: MIT OR Apache-2.0

//! Trait interfaces for interacting with data storage layers.
use std::fmt::Debug;

use p2panda_auth::traits::Conditions;

use crate::OperationId;
use crate::space::SpaceState;
use crate::traits::SpaceId;
use crate::types::AuthGroupState;

/// Methods for setting and fetching space state.
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

    fn spaces_ids(&self) -> impl Future<Output = Result<Vec<ID>, Self::Error>>;

    fn set_space(
        &mut self,
        id: &ID,
        y: SpaceState<ID, M, C>,
    ) -> impl Future<Output = Result<(), Self::Error>>;
}

/// Methods for setting and fetching auth state.
pub trait AuthStore<C>
where
    C: Conditions,
{
    type Error: Debug;

    fn auth(&self) -> impl Future<Output = Result<AuthGroupState<C>, Self::Error>>;

    fn set_auth(&mut self, y: &AuthGroupState<C>) -> impl Future<Output = Result<(), Self::Error>>;
}

// @TODO: replace this with existing OperationStore trait from p2panda-store.
/// Methods for setting and fetching messages.
pub trait MessageStore<M> {
    type Error: Debug;

    fn message(&self, id: &OperationId) -> impl Future<Output = Result<Option<M>, Self::Error>>;

    fn set_message(
        &mut self,
        id: &OperationId,
        message: &M,
    ) -> impl Future<Output = Result<(), Self::Error>>;
}
