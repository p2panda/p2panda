// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;

use p2panda_auth::traits::Conditions;
use p2panda_spaces::event::Event;
use p2panda_spaces::forge::Forge;
use p2panda_spaces::manager::Manager;
use p2panda_spaces::message::{AuthoredMessage, SpacesMessage};
use p2panda_spaces::store::{AuthStore, KeyStore, MessageStore, SpaceStore};
use p2panda_spaces::traits::SpaceId;
use p2panda_spaces::types::AuthResolver;

use crate::Processor;
use crate::spaces::SpacesError;
use crate::utils::AsyncBuffer;

pub struct Spaces<I, S, F, T, C, RS> {
    manager: Manager<I, S, F, T, C, RS>,
    queue: AsyncBuffer<Event<I, C>>,
}

impl<I, S, F, T, C, RS> Spaces<I, S, F, T, C, RS>
where
    I: SpaceId,
    S: SpaceStore<I, T, C> + KeyStore + AuthStore<C> + MessageStore<T> + Debug,
    F: Forge<I, T, C> + Debug,
    T: AuthoredMessage + SpacesMessage<I, C>,
    C: Conditions,
    RS: AuthResolver<C> + Debug,
{
    pub(crate) fn new(manager: Manager<I, S, F, T, C, RS>) -> Self {
        Self {
            manager,
            queue: AsyncBuffer::new(),
        }
    }

    // @TODO: Should we wrap the manager with a "filtered" API, otherwise we would expose the
    // "process" method to the user ..?
    pub fn manager(&self) -> Manager<I, S, F, T, C, RS> {
        self.manager.clone()
    }
}

impl<I, S, F, T, C, RS> Processor<T> for Spaces<I, S, F, T, C, RS>
where
    I: SpaceId,
    S: SpaceStore<I, T, C> + KeyStore + AuthStore<C> + MessageStore<T> + Debug,
    F: Forge<I, T, C> + Debug,
    T: AuthoredMessage + SpacesMessage<I, C>,
    C: Conditions,
    RS: AuthResolver<C> + Debug,
{
    type Output = Event<I, C>;

    type Error = SpacesError<I, S, F, T, C, RS>;

    async fn process(&self, input: T) -> Result<(), Self::Error> {
        let events = self.manager.process(&input).await?;
        self.queue.extend(events).await;
        Ok(())
    }

    async fn next(&self) -> Result<Self::Output, Self::Error> {
        Ok(self.queue.pop().await)
    }
}
