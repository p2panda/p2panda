// SPDX-License-Identifier: MIT OR Apache-2.0

use std::borrow::Borrow;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::marker::PhantomData;

use p2panda_core::{Extensions, Hash, LogId, Operation, PublicKey, SeqNum};
use p2panda_store::Transaction;
use p2panda_store::logs::LogStore;
use p2panda_store::operations::OperationStore;
use p2panda_store::topics::TopicStore;
use tokio::sync::Notify;

use crate::Processor;
use crate::ingest::args::IngestArgs;
use crate::ingest::operation::{IngestError, ingest_operation};

pub struct Ingest<S, T, L, E, TP> {
    store: S,
    notify: Notify,
    queue: RefCell<VecDeque<(T, IngestResult)>>,
    _marker: PhantomData<(L, E, TP)>,
}

impl<S, T, L, E, TP> Ingest<S, T, L, E, TP>
where
    S: Transaction
        + OperationStore<Operation<E>, Hash, L>
        + LogStore<Operation<E>, PublicKey, L, SeqNum, Hash>
        + TopicStore<TP, PublicKey, L>,
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

impl<S, T, L, E, TP> Processor<T> for Ingest<S, T, L, E, TP>
where
    S: Transaction
        + OperationStore<Operation<E>, Hash, L>
        + LogStore<Operation<E>, PublicKey, L, SeqNum, Hash>
        + TopicStore<TP, PublicKey, L>,
    T: Borrow<Operation<E>> + Borrow<IngestArgs<L, TP>>,
    L: LogId,
    E: Extensions,
{
    type Output = (T, IngestResult);

    type Error = (T, IngestError);

    async fn process(&self, input: T) -> Result<(), Self::Error> {
        let operation: &Operation<E> = input.borrow();
        let args: &IngestArgs<L, TP> = input.borrow();

        let result = ingest_operation(
            &self.store,
            operation,
            &args.log_id,
            &args.topic,
            args.prune_flag,
        )
        .await;

        let result = match result {
            Ok(true) => IngestResult::Inserted,
            Ok(false) => IngestResult::AlreadyExists,
            Err(err) => {
                // Return the input arguments next to the error to allow mapping it back to it's
                // source.
                return Err((input, err));
            }
        };

        self.queue.borrow_mut().push_back((input, result));
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

                let log_id = 0;
                let topic = Topic::new();

                let mut stream = stream::iter(vec![
                    Event {
                        operation: operation_0.clone(),
                        args: IngestArgs {
                            log_id,
                            topic,
                            prune_flag: false,
                        },
                    },
                    Event {
                        operation: operation_1.clone(),
                        args: IngestArgs {
                            log_id,
                            topic,
                            prune_flag: false,
                        },
                    },
                    Event {
                        operation: operation_2.clone(),
                        args: IngestArgs {
                            log_id,
                            topic,
                            prune_flag: false,
                        },
                    },
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
