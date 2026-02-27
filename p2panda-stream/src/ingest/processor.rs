// SPDX-License-Identifier: MIT OR Apache-2.0

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

// TODO: Instead of using this concrete data type here we could introduce generics with trait
// bounds. For this to work we need to make the validation methods generic as well.
//
// See issue: https://github.com/p2panda/p2panda/issues/1038
pub struct IngestArguments<L, E, TP> {
    pub operation: Operation<E>,
    pub log_id: L,
    pub topic: TP,
    pub prune_flag: bool,
}

pub struct Ingest<S, L, E, TP> {
    store: S,
    notify: Notify,
    queue: RefCell<VecDeque<Operation<E>>>,
    _marker: PhantomData<(L, TP)>,
}

impl<S, L, E, TP> Ingest<S, L, E, TP>
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

impl<S, L, E, TP> Processor<IngestArguments<L, E, TP>> for Ingest<S, L, E, TP>
where
    S: Transaction
        + OperationStore<Operation<E>, Hash, L>
        + LogStore<Operation<E>, PublicKey, L, SeqNum, Hash>
        + TopicStore<TP, PublicKey, L>,
    L: LogId,
    E: Extensions,
{
    type Output = Operation<E>;

    type Error = IngestError;

    async fn process(&self, args: IngestArguments<L, E, TP>) -> Result<(), Self::Error> {
        ingest_operation(
            &self.store,
            args.operation.clone(),
            args.log_id,
            args.topic,
            args.prune_flag,
        )
        .await?;

        self.queue.borrow_mut().push_back(args.operation);
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
    use futures_util::stream;
    use p2panda_core::Topic;
    use p2panda_core::test_utils::TestLog;
    use p2panda_store::SqliteStore;
    use tokio::task;
    use tokio_stream::StreamExt;

    use crate::StreamLayerExt;

    use super::{Ingest, IngestArguments};

    #[tokio::test]
    async fn ingest_incoming_operations() {
        let log = TestLog::new();
        let local = task::LocalSet::new();

        local
            .run_until(async move {
                let store = SqliteStore::temporary().await;
                let ingest = Ingest::new(store);

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

                let operation = stream.next().await.unwrap().unwrap();
                assert_eq!(operation, operation_0);

                let operation = stream.next().await.unwrap().unwrap();
                assert_eq!(operation, operation_1);

                let operation = stream.next().await.unwrap().unwrap();
                assert_eq!(operation, operation_2);
            })
            .await;
    }
}
