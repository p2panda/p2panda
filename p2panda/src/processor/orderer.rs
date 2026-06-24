// SPDX-License-Identifier: MIT OR Apache-2.0

use std::cell::RefCell;
use std::collections::VecDeque;
use std::marker::PhantomData;

use p2panda_core::traits::Digest;
use p2panda_core::{Hash, LogId, Operation};
use p2panda_store::Transaction;
use p2panda_store::operations::OperationStore;
use p2panda_store::orderer::OrdererStore;
use p2panda_store::processor::ProcessorStore;
use p2panda_stream::Processor;
use p2panda_stream::orderer::{CausalOrderer, Ordering};
use thiserror::Error;
use tokio::sync::{Mutex, Notify};

use crate::processor::ProcessorStatus;
use crate::processor::event::{Event, EventMetadata};

#[derive(Clone, Debug)]
pub enum OrdererResult {
    Processed,
    Ignored,
}

pub struct Orderer<S, T, L, E, TP> {
    inner: Mutex<CausalOrderer<Hash, S>>,
    store: S,
    notify: Notify,
    #[allow(clippy::type_complexity)]
    queue: RefCell<VecDeque<(Event<L, E, TP>, OrdererResult)>>,
    _marker: PhantomData<(T, L, E, TP)>,
}

impl<S, T, L, E, TP> Orderer<S, T, L, E, TP>
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

impl<S, T, L, E, TP> Processor<Event<L, E, TP>> for Orderer<S, T, L, E, TP>
where
    S: Transaction
        + OrdererStore<Hash>
        + OperationStore<Operation<E>, Hash>
        + ProcessorStore<EventMetadata<L, TP>>,
    L: LogId,
    TP: Clone,
    E: Clone,
{
    type Output = (Event<L, E, TP>, OrdererResult);

    type Error = (Option<Event<L, E, TP>>, OrdererError);

    async fn process(&self, input: Event<L, E, TP>) -> Result<(), Self::Error> {
        // Only process the operation if it was successfully ingested.
        if let ProcessorStatus::Completed(_) = input.ingest {
            let inner = self.inner.lock().await;

            let permit = match self.store.begin().await {
                Ok(permit) => permit,
                Err(err) => return Err((Some(input), OrdererError::Transaction(err.to_string()))),
            };

            if let Err(err) = inner.process(input.hash(), &input.dependencies()[..]).await {
                return Err((Some(input), OrdererError::OrdererStore(err.to_string())));
            };

            let metadata: EventMetadata<L, TP> = input.clone().into();

            if let Err(err) = self.store.set_event(&input.hash(), &metadata).await {
                return Err((Some(input), OrdererError::ProcessorStore(err.to_string())));
            };

            self.store
                .commit(permit)
                .await
                .map_err(|err| (Some(input), OrdererError::Transaction(err.to_string())))?;
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
            // First check to see if there are any ignored events which can be returned.
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

                let EventMetadata {
                    log_id,
                    topic,
                    prune_flag,
                    spaces_args,
                    ingest,
                } = match self.store.get_event(&id).await {
                    Ok(Some(metadata)) => metadata,
                    Ok(None) => return Err((None, OrdererError::StoreInconsistency(id))),
                    Err(err) => return Err((None, OrdererError::ProcessorStore(err.to_string()))),
                };

                let mut event = Event::new(operation, log_id, topic, prune_flag, spaces_args);
                event.ingest = ingest;

                return Ok((event, OrdererResult::Processed));
            }

            self.store
                .commit(permit)
                .await
                .map_err(|err| (None, OrdererError::Transaction(err.to_string())))?;

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
