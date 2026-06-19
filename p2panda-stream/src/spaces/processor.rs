// SPDX-License-Identifier: MIT OR Apache-2.0

use std::borrow::Borrow;
use std::cell::RefCell;
use std::collections::VecDeque;

use p2panda_auth::traits::Conditions;
use p2panda_spaces::manager::{Manager, ManagerError};
use p2panda_spaces::space::SpacesState;
use p2panda_spaces::{AuthMessage, Event, StrongRemoveResolver};
use p2panda_spaces::{Forge, SpacesArgs};
use p2panda_store::Transaction;
use p2panda_store::groups::GroupsStore;
use p2panda_store::key_registry::KeyRegistryStore;
use p2panda_store::key_secrets::KeySecretsStore;
use p2panda_store::spaces::{SpacesMessageStore, SpacesStore};
use tokio::sync::Notify;
use tracing::trace;

use crate::Processor;
use crate::spaces::SpacesProcessorArgs;

pub type SpacesManager<S, F, C> = Manager<S, F, C, StrongRemoveResolver<C>>;

pub type SpacesManagerError<F, C> = ManagerError<F, C, StrongRemoveResolver<C>>;

#[derive(Clone)]
pub enum SpacesResult<C> {
    Processed { events: Vec<Event<C>> },
    Ignored,
}

impl<C> SpacesResult<C> {
    pub fn was_processed(self) -> bool {
        match self {
            Self::Processed { .. } => true,
            Self::Ignored => false,
        }
    }
}

/// Processor for spaces operations.
pub struct Spaces<T, S, F, C> {
    manager: SpacesManager<S, F, C>,
    notify: Notify,
    queue: RefCell<VecDeque<(T, SpacesResult<C>)>>,
}

impl<T, S, F, C> Spaces<T, S, F, C> {
    pub fn new(manager: SpacesManager<S, F, C>) -> Self {
        Self {
            manager,
            notify: Notify::new(),
            queue: RefCell::new(VecDeque::new()),
        }
    }
}

impl<T, S, F, C> Processor<T> for Spaces<T, S, F, C>
where
    T: Borrow<SpacesProcessorArgs<C>>,
    S: Clone
        + Transaction
        + SpacesStore<SpacesState<C>>
        + SpacesMessageStore<SpacesArgs<C>>
        + GroupsStore<AuthMessage<C>, C>
        + KeyRegistryStore
        + KeySecretsStore,
    F: Forge<C>,
    C: Conditions,
{
    type Output = (T, SpacesResult<C>);

    type Error = (T, SpacesManagerError<F, C>);

    async fn process(&self, input: T) -> Result<(), Self::Error> {
        let input_args: &SpacesProcessorArgs<C> = input.borrow();

        let result = if let SpacesProcessorArgs::Process { msg } = input_args {
            let (groups_y, space_y, events) = match self.manager.process(msg).await {
                Ok(result) => result,
                Err(err) => return Err((input, err)),
            };

            if let Some(_groups_y) = groups_y {
                // @TODO: persist groups state.
            }

            if let Some(_space_y) = space_y {
                // @TODO: persist space state.
            }

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
