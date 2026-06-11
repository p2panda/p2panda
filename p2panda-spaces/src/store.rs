// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_auth::traits::Conditions;
use thiserror::Error;

use crate::traits::{AuthStore, MessageStore, SpaceId, SpacesStore};

#[derive(Debug, Error)]
pub enum StoreError<ID, S, M, C>
where
    ID: SpaceId,
    S: SpacesStore<ID, M, C> + AuthStore<C> + MessageStore<M>,
    C: Conditions,
{
    #[error("{0}")]
    Auth(<S as AuthStore<C>>::Error),

    #[error("{0}")]
    Message(<S as MessageStore<M>>::Error),

    #[error("{0}")]
    Spaces(<S as SpacesStore<ID, M, C>>::Error),
}
