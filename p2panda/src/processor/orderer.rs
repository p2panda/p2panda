// SPDX-License-Identifier: MIT OR Apache-2.0

use std::borrow::Borrow;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::marker::PhantomData;

use p2panda_core::traits::Digest;
use p2panda_core::{Extensions, Hash, Operation};
use p2panda_store::Transaction;
use p2panda_store::operations::OperationStore;
use p2panda_store::orderer::OrdererStore;
use p2panda_store::processor::ProcessorStore;
use p2panda_stream::Processor;
use p2panda_stream::orderer::CausalOrderer;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{Mutex, Notify};

pub trait OrdererMetadata<E> {
    /// Metadata attached to an input event we want to persist in database next to ordering info.
    type Metadata: Serialize + for<'de> Deserialize<'de>;

    /// Extract metadata from input.
    fn metadata(&self) -> Self::Metadata;

    /// Re-construct input from operation and persisted metadata.
    fn from_operation(operation: Operation<E>, meta: Self::Metadata) -> Self;
}

#[derive(Clone, Default, Debug)]
pub enum OrdererArgs {
    Process {
        dependencies: Vec<Hash>,
    },
    #[default]
    Ignore,
}

#[derive(Copy, Clone, Debug)]
pub enum OrdererResult {
    /// Item was buffered and is now in "pending" state.
    Pending,

    /// Item was buffered by orderer and is now marked as "ready" to be finally forwarded in correct
    /// order.
    Ready,

    /// Item was ignored as specified in input arguments.
    Ignored,
}

impl OrdererResult {
    pub fn is_pending(&self) -> bool {
        matches!(self, OrdererResult::Pending)
    }
}

pub struct Orderer<S, T, E> {
    inner: Mutex<CausalOrderer<Hash, S>>,
    store: S,
    notify: Notify,
    queue: RefCell<VecDeque<(T, OrdererResult)>>,
    _marker: PhantomData<E>,
}

impl<S, T, E> Orderer<S, T, E>
where
    S: Clone + Transaction + OrdererStore<Hash> + OperationStore<Operation<E>, Hash>,
{
    pub fn new(store: S) -> Self {
        let inner = CausalOrderer::new(store.clone());

        Self {
            inner: Mutex::new(inner),
            store,
            notify: Notify::new(),
            queue: RefCell::new(VecDeque::new()),
            _marker: PhantomData,
        }
    }
}

impl<S, T, E> Processor<T> for Orderer<S, T, E>
where
    S: Transaction
        + OrdererStore<Hash>
        + OperationStore<Operation<E>, Hash>
        + ProcessorStore<T::Metadata>,
    T: OrdererMetadata<E> + Borrow<OrdererArgs> + Digest<Hash>,
    E: Extensions,
{
    type Output = (T, OrdererResult);

    type Error = (Option<T>, OrdererError);

    async fn process(&self, input: T) -> Result<(), Self::Error> {
        let args = input.borrow();

        if let OrdererArgs::Process { dependencies } = args {
            let inner = self.inner.lock().await;

            let permit = match self.store.begin().await {
                Ok(permit) => permit,
                Err(err) => return Err((Some(input), OrdererError::Transaction(err.to_string()))),
            };

            if let Err(err) = inner.process(input.hash(), dependencies).await {
                return Err((Some(input), OrdererError::OrdererStore(err.to_string())));
            };

            if let Err(err) = self.store.set_event(&input.hash(), &input.metadata()).await {
                return Err((Some(input), OrdererError::ProcessorStore(err.to_string())));
            };

            if let Err(err) = self.store.commit(permit).await {
                return Err((Some(input), OrdererError::Transaction(err.to_string())));
            }

            self.queue
                .borrow_mut()
                .push_back((input, OrdererResult::Pending));
        } else {
            self.queue
                .borrow_mut()
                .push_back((input, OrdererResult::Ignored));
        }

        self.notify.notify_one(); // Wake up any pending next call

        Ok(())
    }

    async fn next(&self) -> Result<Self::Output, Self::Error> {
        loop {
            // First check to see if there are any ignored or pending events which can be returned.
            if let Some((event, result)) = self.queue.borrow_mut().pop_front() {
                return Ok((event, result));
            }

            let permit = self
                .store
                .begin()
                .await
                .map_err(|err| (None, OrdererError::Transaction(err.to_string())))?;

            let inner = self.inner.lock().await;

            if let Some(id) = inner
                .next()
                .await
                .map_err(|err| (None, OrdererError::OrdererStore(err.to_string())))?
            {
                self.store
                    .commit(permit)
                    .await
                    .map_err(|err| (None, OrdererError::Transaction(err.to_string())))?;

                let operation = match self
                    .store
                    .get_operation(&id)
                    .await
                    .map_err(|err| OrdererError::OperationStore(err.to_string()))
                {
                    Ok(Some(operation)) => operation,
                    Ok(None) => return Err((None, OrdererError::StoreInconsistency(id))),
                    Err(err) => return Err((None, err)),
                };

                let metadata = match self.store.get_event(&id).await {
                    Ok(Some(metadata)) => metadata,
                    Ok(None) => return Err((None, OrdererError::StoreInconsistency(id))),
                    Err(err) => return Err((None, OrdererError::ProcessorStore(err.to_string()))),
                };

                return Ok((T::from_operation(operation, metadata), OrdererResult::Ready));
            }

            self.store
                .commit(permit)
                .await
                .map_err(|err| (None, OrdererError::Transaction(err.to_string())))?;

            drop(inner);

            self.notify.notified().await;
        }
    }
}

#[derive(Clone, Debug, Error)]
pub enum OrdererError {
    #[error("could not find item with id {0} in operation store")]
    StoreInconsistency(Hash),

    #[error("{0}")]
    OrdererStore(String),

    #[error("{0}")]
    OperationStore(String),

    #[error("{0}")]
    ProcessorStore(String),

    #[error("{0}")]
    Transaction(String),
}
