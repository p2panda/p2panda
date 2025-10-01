// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;
use std::marker::PhantomData;

use p2panda_auth::traits::Conditions;
use p2panda_encryption::Rng;
use p2panda_spaces::forge::Forge;
use p2panda_spaces::manager::{Manager, ManagerError};
use p2panda_spaces::message::{AuthoredMessage, SpacesMessage};
use p2panda_spaces::store::{AuthStore, KeyStore, MessageStore, SpaceStore};
use p2panda_spaces::traits::SpaceId;
use p2panda_spaces::types::AuthResolver;

use crate::spaces::Spaces;

pub type SpacesError<I, S, F, T, C, RS> = ManagerError<I, S, F, T, C, RS>;

pub struct SpacesBuilder<I, M, C> {
    rng: Option<Rng>,
    _marker: PhantomData<(I, M, C)>,
}

impl<T, I, C> SpacesBuilder<T, I, C>
where
    T: AuthoredMessage + SpacesMessage<I, C>,
    I: SpaceId,
    C: Conditions,
{
    pub fn new() -> Self {
        SpacesBuilder {
            rng: None,
            _marker: PhantomData,
        }
    }

    pub fn with_rng(mut self, rng: Rng) -> Self {
        self.rng = Some(rng);
        self
    }

    pub fn build<S, F, RS>(
        self,
        store: S,
        forge: F,
    ) -> Result<Spaces<I, S, F, T, C, RS>, SpacesError<I, S, F, T, C, RS>>
    where
        S: SpaceStore<I, T, C> + KeyStore + AuthStore<C> + MessageStore<T> + Debug,
        F: Forge<I, T, C> + Debug,
        RS: AuthResolver<C> + Debug,
    {
        let rng = self.rng.unwrap_or_default();
        let manager = Manager::<I, S, F, T, C, RS>::new(store, forge, rng)?;
        Ok(Spaces::new(manager))
    }
}
