// SPDX-License-Identifier: MIT OR Apache-2.0

use std::borrow::Borrow;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt::Debug;

use p2panda_auth::traits::Conditions;
use p2panda_spaces::manager::{Manager, ManagerError};
use p2panda_spaces::traits::{
    AuthStore, AuthoredMessage, Forge, KeyRegistryStore, KeySecretStore, MessageStore, SpaceId,
    SpacesStore,
};
use p2panda_spaces::{Event, SpacesArgs as SpacesMessageArgs, StrongRemoveResolver};
use tokio::sync::Notify;
use tracing::trace;

use crate::Processor;
use crate::spaces::SpacesArgs;

pub type SpacesManager<ID, S, K, F, C> = Manager<ID, S, K, F, C, StrongRemoveResolver<C>>;
pub type SpacesManagerError<ID, S, K, F, C> = ManagerError<ID, S, K, F, C, StrongRemoveResolver<C>>;

#[derive(Clone, Debug)]
pub enum SpacesResult<ID, C> {
    Processed { events: Vec<Event<ID, C>> },
    Ignored,
}

impl<ID, C> SpacesResult<ID, C> {
    pub fn was_processed(self) -> bool {
        match self {
            Self::Processed { .. } => true,
            Self::Ignored => false,
        }
    }
}

/// Processor for spaces operations.
pub struct Spaces<T, ID, S, K, F, C> {
    manager: SpacesManager<ID, S, K, F, C>,
    notify: Notify,
    queue: RefCell<VecDeque<(T, SpacesResult<ID, C>)>>,
}

impl<T, ID, S, K, F, C> Spaces<T, ID, S, K, F, C> {
    pub fn new(manager: SpacesManager<ID, S, K, F, C>) -> Self {
        Self {
            manager,
            notify: Notify::new(),
            queue: RefCell::new(VecDeque::new()),
        }
    }
}

impl<T, ID, S, K, F, C> Processor<T> for Spaces<T, ID, S, K, F, C>
where
    T: Borrow<SpacesArgs<ID, C>>,
    ID: SpaceId,
    S: SpacesStore<ID, C> + AuthStore<C> + MessageStore<F::Message> + Debug,
    K: KeyRegistryStore + KeySecretStore + Debug,
    F: Forge<ID, C> + Debug,
    F::Message: AuthoredMessage + Borrow<SpacesMessageArgs<ID, C>>,
    C: Conditions,
{
    type Output = (T, SpacesResult<ID, C>);

    type Error = (T, SpacesManagerError<ID, S, K, F, C>);

    async fn process(&self, input: T) -> Result<(), Self::Error> {
        let input_args: &SpacesArgs<ID, C> = input.borrow();

        let result = if let SpacesArgs::Process { msg } = input_args {
            let events = match self.manager.process(msg).await {
                Ok(events) => events,
                Err(err) => return Err((input, err)),
            };

            (input, SpacesResult::Processed { events })
        } else {
            trace!("ignore non-spaces operation");
            (input, SpacesResult::Ignored)
        };

        self.queue.borrow_mut().push_back(result);
        self.notify.notify_one();

        Ok(())
    }

    async fn next(&self) -> Result<Self::Output, Self::Error> {
        loop {
            if let Some(item) = self.queue.borrow_mut().pop_front() {
                return Ok(item);
            }

            self.notify.notified().await;
        }
    }
}
