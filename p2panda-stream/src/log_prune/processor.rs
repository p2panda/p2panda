// SPDX-License-Identifier: MIT OR Apache-2.0

use std::borrow::Borrow;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::marker::PhantomData;

use p2panda_core::{Extensions, Hash, LogId, Operation, PublicKey, SeqNum};
use p2panda_store::logs::LogStore;
use thiserror::Error;
use tokio::sync::Notify;

use crate::Processor;
use crate::log_prune::LogPruneArgs;

pub struct LogPrune<S, T, L, E> {
    store: S,
    notify: Notify,
    queue: RefCell<VecDeque<(T, LogPruneResult)>>,
    _marker: PhantomData<(L, E)>,
}

impl<S, T, L, E> LogPrune<S, T, L, E>
where
    S: LogStore<Operation<E>, PublicKey, L, SeqNum, Hash>,
    L: LogId,
    E: Extensions,
{
    pub fn new(store: S) -> Self {
        Self {
            store,
            notify: Notify::new(),
            queue: RefCell::new(VecDeque::new()),
            _marker: PhantomData,
        }
    }
}

impl<S, T, L, E> Processor<T> for LogPrune<S, T, L, E>
where
    S: LogStore<Operation<E>, PublicKey, L, SeqNum, Hash>,
    T: Borrow<LogPruneArgs<PublicKey, L, SeqNum>>,
    L: LogId,
    E: Extensions,
{
    type Output = (T, LogPruneResult);

    type Error = (T, LogPruneError);

    async fn process(&self, input: T) -> Result<(), Self::Error> {
        let args: &LogPruneArgs<PublicKey, L, SeqNum> = input.borrow();

        let result = if let LogPruneArgs::PruneEntriesUntil {
            author,
            log_id,
            seq_num,
        } = args
        {
            match self.store.prune_entries(author, log_id, seq_num).await {
                Ok(num_entries) => (input, LogPruneResult::Pruned { num_entries }),
                Err(err) => {
                    // Return the input arguments next to the error to allow mapping it back to
                    // it's source.
                    return Err((input, LogPruneError::StoreError(err.to_string())));
                }
            }
        } else {
            (input, LogPruneResult::Noop)
        };

        self.queue.borrow_mut().push_back(result);
        self.notify.notify_one(); // Wake up any pending recv.

        Ok(())
    }

    async fn next(&self) -> Result<Self::Output, Self::Error> {
        loop {
            if let Some(item) = self.queue.borrow_mut().pop_front() {
                return Ok(item);
            }

            // Wait for notification that an item was added.
            self.notify.notified().await;
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LogPruneResult {
    Noop,
    Pruned { num_entries: u64 },
}

#[derive(Clone, Debug, Error)]
pub enum LogPruneError {
    /// Critical storage failure occurred. This is usually a reason to panic.
    #[error("critical storage failure: {0}")]
    StoreError(String),
}
