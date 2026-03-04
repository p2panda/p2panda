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
use crate::ingest::operation::{IngestError, ingest_operation};
use crate::ingest::traits::IngestArgs;

pub struct Ingest<S, T, L, E, TP> {
    store: S,
    notify: Notify,
    queue: RefCell<VecDeque<T>>,
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
    // TODO: remove Clone after https://github.com/p2panda/p2panda/issues/1040
    T: IngestArgs<L, TP, E> + Clone,
    L: LogId,
    E: Extensions,
{
    type Output = T;

    type Error = (T, IngestError);

    async fn process(&self, args: T) -> Result<(), Self::Error> {
        let result = ingest_operation(
            &self.store,
            args.operation().borrow(),
            args.log_id(),
            args.topic(),
            args.prune_flag(),
        )
        .await;

        // Return the input arguments next to the error to allow mapping it back to it's source.
        if let Err(err) = result {
            return Err((args, err));
        }

        self.queue.borrow_mut().push_back(args);
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
    use crate::ingest::traits::IngestArgs;

    use super::Ingest;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct IngestArguments {
        pub operation: Operation,
        pub log_id: usize,
        pub topic: Topic,
        pub prune_flag: bool,
    }

    impl IngestArgs<usize, Topic, ()> for IngestArguments {
        fn log_id(&self) -> usize {
            self.log_id
        }

        fn topic(&self) -> Topic {
            self.topic
        }

        fn prune_flag(&self) -> bool {
            self.prune_flag
        }

        fn operation(&self) -> impl Borrow<Operation> {
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
                let ingest: Ingest<SqliteStore<'_>, IngestArguments, _, _, _> = Ingest::new(store);

                let operation_0 = log.operation(b"Hi", ());
                let operation_1 = log.operation(b"Ha", ());
                let operation_2 = log.operation(b"Ho", ());

                let log_id = 0;
                let topic = Topic::new();

                let mut stream = stream::iter(vec![
                    IngestArguments {
                        operation: operation_0.clone(),
                        log_id,
                        topic,
                        prune_flag: false,
                    },
                    IngestArguments {
                        operation: operation_1.clone(),
                        log_id,
                        topic,
                        prune_flag: false,
                    },
                    IngestArguments {
                        operation: operation_2.clone(),
                        log_id,
                        topic,
                        prune_flag: false,
                    },
                ])
                .layer(ingest);

                let args = stream.next().await.unwrap().unwrap();
                assert_eq!(args.operation, operation_0);

                let args = stream.next().await.unwrap().unwrap();
                assert_eq!(args.operation, operation_1);

                let args = stream.next().await.unwrap().unwrap();
                assert_eq!(args.operation, operation_2);
            })
            .await;
    }
}
