// SPDX-License-Identifier: MIT OR Apache-2.0

use std::borrow::Borrow;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::marker::PhantomData;

use p2panda_core::{Extensions, Hash, LogId, Operation, OperationError, SeqNum, VerifyingKey};
use p2panda_store::Transaction;
use p2panda_store::logs::LogStore;
use p2panda_store::operations::OperationStore;
use p2panda_store::topics::TopicStore;
use tokio::sync::Notify;

use crate::Processor;
use crate::ingest::args::IngestArgs;
use crate::ingest::operation::{IngestError, ingest_operation};

// Live operations from concurrent sessions should only be separated by a small scheduling skew.
// Keeping this window deliberately tight prevents a truly missing predecessor from suppressing an
// author's entire stream indefinitely; overflow is surfaced so the sync layer can recover.
const MAX_PENDING_OPERATIONS: usize = 64;

type IngestOutput<T> = Result<(T, IngestResult), (T, IngestError)>;

pub struct Ingest<S, T, L, E, TP> {
    store: S,
    notify: Notify,
    queue: RefCell<VecDeque<IngestOutput<T>>>,
    pending: RefCell<VecDeque<T>>,
    _marker: PhantomData<(L, E, TP)>,
}

impl<S, T, L, E, TP> Ingest<S, T, L, E, TP>
where
    S: Transaction
        + OperationStore<Operation<E>, Hash>
        + LogStore<Operation<E>, VerifyingKey, L, SeqNum, Hash>
        + TopicStore<TP, VerifyingKey, L>,
    L: LogId,
    E: Extensions,
{
    pub fn new(store: S) -> Self {
        Self {
            store,
            notify: Notify::new(),
            queue: RefCell::new(VecDeque::new()),
            pending: RefCell::new(VecDeque::new()),
            _marker: PhantomData,
        }
    }

    fn enqueue(&self, output: IngestOutput<T>) {
        self.queue.borrow_mut().push_back(output);
        self.notify.notify_one();
    }

    fn can_defer(input: &T, error: &IngestError) -> bool
    where
        T: Borrow<Operation<E>>,
    {
        match error {
            IngestError::InvalidOperation(OperationError::SeqNumNonIncremental(expected, found)) => {
                found > expected
            }
            IngestError::InvalidOperation(OperationError::BacklinkMissing) => {
                let operation: &Operation<E> = input.borrow();
                operation.header.seq_num > 0
            }
            _ => false,
        }
    }

    async fn try_ingest(&self, input: &T) -> Result<IngestResult, IngestError>
    where
        T: Borrow<Operation<E>> + Borrow<IngestArgs<L, TP>>,
    {
        let operation: &Operation<E> = input.borrow();
        let args: &IngestArgs<L, TP> = input.borrow();

        match ingest_operation(
            &self.store,
            operation,
            &args.log_id,
            &args.topic,
            args.prune_flag,
        )
        .await?
        {
            true => Ok(IngestResult::Inserted),
            false => Ok(IngestResult::AlreadyExists),
        }
    }

    async fn drain_pending(&self)
    where
        T: Borrow<Operation<E>> + Borrow<IngestArgs<L, TP>>,
    {
        loop {
            let mut pending = self.pending.replace(VecDeque::new());
            if pending.is_empty() {
                return;
            }

            let mut deferred = VecDeque::new();
            let mut made_progress = false;

            while let Some(input) = pending.pop_front() {
                match self.try_ingest(&input).await {
                    Ok(result) => {
                        made_progress = true;
                        self.enqueue(Ok((input, result)));
                    }
                    Err(error) if Self::can_defer(&input, &error) => {
                        deferred.push_back(input);
                    }
                    Err(error) => self.enqueue(Err((input, error))),
                }
            }

            // `process` and `drain_pending` run on the local processor task, but preserve any
            // operations which might have been queued re-entrantly while awaits yielded.
            let mut newly_pending = self.pending.borrow_mut();
            deferred.append(&mut newly_pending);
            drop(newly_pending);
            self.pending.replace(deferred);

            if !made_progress {
                return;
            }
        }
    }
}

impl<S, T, L, E, TP> Processor<T> for Ingest<S, T, L, E, TP>
where
    S: Transaction
        + OperationStore<Operation<E>, Hash>
        + LogStore<Operation<E>, VerifyingKey, L, SeqNum, Hash>
        + TopicStore<TP, VerifyingKey, L>,
    T: Borrow<Operation<E>> + Borrow<IngestArgs<L, TP>>,
    L: LogId,
    E: Extensions,
{
    type Output = (T, IngestResult);

    type Error = (T, IngestError);

    async fn process(&self, input: T) -> Result<(), Self::Error> {
        match self.try_ingest(&input).await {
            Ok(result) => {
                self.enqueue(Ok((input, result)));
                self.drain_pending().await;
            }
            Err(error)
                if Self::can_defer(&input, &error)
                    && self.pending.borrow().len() < MAX_PENDING_OPERATIONS =>
            {
                self.pending.borrow_mut().push_back(input);
            }
            Err(error) => self.enqueue(Err((input, error))),
        }

        Ok(())
    }

    async fn next(&self) -> Result<Self::Output, Self::Error> {
        loop {
            // Register before checking the queue so a notification cannot be lost between the
            // empty check and awaiting it.
            let notified = self.notify.notified();
            if let Some(item) = self.queue.borrow_mut().pop_front() {
                return item;
            }
            notified.await;
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IngestResult {
    AlreadyExists,
    Inserted,
}

#[cfg(test)]
mod tests {
    use std::borrow::Borrow;

    use futures_util::stream;
    use p2panda_core::test_utils::TestLog;
    use p2panda_core::{Operation, Topic};
    use p2panda_store::SqliteStore;
    use tokio::task;
    use tokio_stream::StreamExt;

    use crate::StreamLayerExt;
    use crate::ingest::args::IngestArgs;

    use super::Ingest;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Event {
        pub operation: Operation,
        pub args: IngestArgs<usize, Topic>,
    }

    impl Borrow<IngestArgs<usize, Topic>> for Event {
        fn borrow(&self) -> &IngestArgs<usize, Topic> {
            &self.args
        }
    }

    impl Borrow<Operation> for Event {
        fn borrow(&self) -> &Operation {
            &self.operation
        }
    }

    fn event(operation: Operation, log_id: usize, topic: Topic) -> Event {
        Event {
            operation,
            args: IngestArgs {
                log_id,
                topic,
                prune_flag: false,
            },
        }
    }

    #[tokio::test]
    async fn ingest_incoming_operations() {
        let log = TestLog::new();
        let local = task::LocalSet::new();

        local
            .run_until(async move {
                let store = SqliteStore::temporary().await;
                let ingest: Ingest<SqliteStore, Event, _, _, _> = Ingest::new(store);

                let operation_0 = log.operation(b"Hi", ());
                let operation_1 = log.operation(b"Ha", ());
                let operation_2 = log.operation(b"Ho", ());
                let topic = Topic::random();

                let mut stream = stream::iter(vec![
                    event(operation_0.clone(), 0, topic),
                    event(operation_1.clone(), 0, topic),
                    event(operation_2.clone(), 0, topic),
                ])
                .layer(ingest);

                let (event, _) = stream.next().await.unwrap().unwrap();
                assert_eq!(event.operation, operation_0);

                let (event, _) = stream.next().await.unwrap().unwrap();
                assert_eq!(event.operation, operation_1);

                let (event, _) = stream.next().await.unwrap().unwrap();
                assert_eq!(event.operation, operation_2);
            })
            .await;
    }

    #[tokio::test]
    async fn defers_future_operation_until_predecessor_arrives() {
        let log = TestLog::new();
        let local = task::LocalSet::new();

        local
            .run_until(async move {
                let store = SqliteStore::temporary().await;
                let ingest: Ingest<SqliteStore, Event, _, _, _> = Ingest::new(store);

                let operation_0 = log.operation(b"0", ());
                let operation_1 = log.operation(b"1", ());
                let operation_2 = log.operation(b"2", ());
                let topic = Topic::random();

                // Concurrent sync sessions can interleave one author's live operations this way.
                let mut stream = stream::iter(vec![
                    event(operation_0.clone(), 0, topic),
                    event(operation_2.clone(), 0, topic),
                    event(operation_1.clone(), 0, topic),
                ])
                .layer(ingest);

                let (event, _) = stream.next().await.unwrap().unwrap();
                assert_eq!(event.operation, operation_0);

                let (event, _) = stream.next().await.unwrap().unwrap();
                assert_eq!(event.operation, operation_1);

                let (event, _) = stream.next().await.unwrap().unwrap();
                assert_eq!(event.operation, operation_2);
            })
            .await;
    }
}
