// SPDX-License-Identifier: MIT OR Apache-2.0

use std::borrow::Borrow;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt::Debug;

use p2panda_auth::traits::Conditions;
use p2panda_encryption::key_manager::PreKeyBundlesState;
use p2panda_encryption::key_registry::KeyRegistryState;
use p2panda_spaces::manager::{Manager, ManagerError};
use p2panda_spaces::traits::{AuthoredMessage, Forge, SpaceId};
use p2panda_spaces::{
    ActorId, Event, GroupsStore, SpacesArgs as SpacesMessageArgs, SpacesMessageStore, SpacesStore,
    StrongRemoveResolver,
};
use p2panda_store::Transaction;
use p2panda_store::key_registry::KeyRegistryStore;
use p2panda_store::key_secrets::KeySecretsStore;
use tokio::sync::Notify;
use tracing::trace;

use crate::Processor;
use crate::spaces::SpacesArgs;

pub type SpacesManager<ID, S, K, F, C> = Manager<ID, S, K, F, C, StrongRemoveResolver<C>>;
pub type SpacesManagerError<ID, F, C> = ManagerError<ID, F, C, StrongRemoveResolver<C>>;

#[derive(Clone)]
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
    S: SpacesStore<ID, C> + SpacesMessageStore<ID, C> + GroupsStore<C> + Transaction,
    K: KeyRegistryStore<KeyRegistryState<ActorId>>
        + KeySecretsStore<PreKeyBundlesState>
        + Transaction,
    F: Forge<ID, C> + Debug,
    F::Message: AuthoredMessage + Borrow<SpacesMessageArgs<ID, C>>,
    C: Conditions,
{
    type Output = (T, SpacesResult<ID, C>);

    type Error = (T, SpacesManagerError<ID, F, C>);

    async fn process(&self, input: T) -> Result<(), Self::Error> {
        let input_args: &SpacesArgs<ID, C> = input.borrow();

        let result = if let SpacesArgs::Process { msg } = input_args {
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
