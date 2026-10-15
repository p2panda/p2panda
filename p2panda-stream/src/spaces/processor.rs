// SPDX-License-Identifier: MIT OR Apache-2.0

use std::borrow::Borrow;
use std::cell::RefCell;
use std::collections::VecDeque;

use p2panda_auth::traits::Conditions;
use p2panda_spaces::Forge;
use p2panda_spaces::manager::Manager;
use p2panda_spaces::{AuthMessage, Event, SpacesStoreState};
use p2panda_store::Transaction;
use p2panda_store::groups::GroupsStore;
use p2panda_store::key_registry::KeyRegistryStore;
use p2panda_store::key_secrets::PreKeyBundlesStore;
use p2panda_store::spaces::{SpacesMessageStore, SpacesStore};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::Notify;

use crate::Processor;
use crate::spaces::SpacesProcessorArgs;

pub type SpacesManager<S, F, C> = Manager<S, F, C>;

#[derive(Clone, Debug, Error)]
#[error("{0}")]
pub struct SpacesError(String);

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
        + SpacesStore<SpacesStoreState<C>>
        + SpacesMessageStore<F::Message>
        + GroupsStore<AuthMessage<C>, C>
        + KeyRegistryStore
        + PreKeyBundlesStore
        + Transaction,
    F: Forge<C>,
    C: Conditions + Serialize + for<'a> Deserialize<'a>,
{
    type Output = (T, SpacesResult<C>);

    type Error = (T, SpacesError);

    async fn process(&self, input: T) -> Result<(), Self::Error> {
        let input_args: &SpacesProcessorArgs<C> = input.borrow();

        let result = match input_args {
            SpacesProcessorArgs::Process { msg } => match self.manager.process(msg).await {
                Ok(events) => (input, SpacesResult::Processed { events }),
                Err(err) => return Err((input, SpacesError(err.to_string()))),
            },
            // For locally created operations the spaces args have already been processed and
            // resulting events are included in the processor args here. All that's needed from the
            // processor is to forward them onto any consumers.
            SpacesProcessorArgs::AlreadyProcessed { events, .. } => {
                let events = events.clone();
                (input, SpacesResult::Processed { events })
            }
            SpacesProcessorArgs::Ignore => (input, SpacesResult::Ignored),
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

#[derive(Clone, Debug)]
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
