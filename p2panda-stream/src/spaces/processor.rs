// SPDX-License-Identifier: MIT OR Apache-2.0

use std::borrow::Borrow;
use std::cell::RefCell;
use std::collections::VecDeque;

use p2panda_auth::traits::Conditions;
use p2panda_core::Hash;
use p2panda_spaces::manager::{GLOBAL_GROUPS_CONTEXT_ID, Manager, ManagerError};
use p2panda_spaces::{AuthMessage, Event, SpacesStoreState};
use p2panda_spaces::{Forge, SpacesArgs};
use p2panda_store::Transaction;
use p2panda_store::groups::GroupsStore;
use p2panda_store::key_registry::KeyRegistryStore;
use p2panda_store::key_secrets::KeySecretsStore;
use p2panda_store::spaces::{SpacesMessageStore, SpacesStore};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::Notify;

use crate::Processor;
use crate::spaces::SpacesProcessorArgs;

pub type SpacesManager<S, F, C> = Manager<S, F, C>;

pub type SpacesManagerError<F, C> = ManagerError<F, C>;

/// Processor for spaces operations.
pub struct Spaces<T, S, F, C> {
    store: S,
    manager: SpacesManager<S, F, C>,
    notify: Notify,
    queue: RefCell<VecDeque<(T, SpacesResult<C>)>>,
}

impl<T, S, F, C> Spaces<T, S, F, C> {
    pub fn new(store: S, manager: SpacesManager<S, F, C>) -> Self {
        Self {
            store,
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
        + SpacesMessageStore<SpacesArgs<C>>
        + GroupsStore<AuthMessage<C>, C>
        + KeyRegistryStore
        + KeySecretsStore
        + Transaction,
    F: Forge<C>,
    C: Conditions + Serialize + for<'a> Deserialize<'a>,
{
    type Output = (T, SpacesResult<C>);

    type Error = (T, SpacesError);

    async fn process(&self, input: T) -> Result<(), Self::Error> {
        let input_args: &SpacesProcessorArgs<C> = input.borrow();

        let result = if let SpacesProcessorArgs::Process { msg } = input_args {
            // Process incoming event.
            let (groups_y, space_y, events) = match self.manager.process(msg).await {
                Ok(result) => result,
                Err(err) => return Err((input, SpacesError::SpacesManager(err.to_string()))),
            };

            // Persist resulting new states into database in one atomic transaction.
            let permit = match self.store.begin().await {
                Ok(permit) => permit,
                Err(err) => return Err((input, SpacesError::Store(err.to_string()))),
            };

            if let Some(y) = groups_y {
                // TODO: Hashing every time when processing feels a bit redundant. We either want to
                // hard-code the hash itself or change the id type in p2panda-store to a string OR
                // have the constant in p2panda-store.
                if let Err(err) = self
                    .store
                    .set_groups_state_tx(Hash::digest(GLOBAL_GROUPS_CONTEXT_ID), &y)
                    .await
                {
                    return Err((input, SpacesError::Store(err.to_string())));
                }
            }

            if let Some(y) = space_y {
                let space_id = y.space_id;

                if let Err(err) = self
                    .store
                    .set_space_state_tx(&space_id, &SpacesStoreState::from(y))
                    .await
                {
                    return Err((input, SpacesError::Store(err.to_string())));
                }
            }

            if let Err(err) = self.store.commit(permit).await {
                return Err((input, SpacesError::Store(err.to_string())));
            }

            (input, SpacesResult::Processed { events })
        } else {
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

#[derive(Clone, Debug, Error)]
pub enum SpacesError {
    /// Error occurred when processing -spaces event in manager.
    #[error("spaces processing error: {0}")]
    SpacesManager(String),

    /// Critical storage failure occurred. This is usually a reason to panic.
    #[error("critical storage failure: {0}")]
    Store(String),
}
