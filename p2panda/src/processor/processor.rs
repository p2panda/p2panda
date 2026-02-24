// SPDX-License-Identifier: MIT OR Apache-2.0

use std::marker::PhantomData;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use futures_util::StreamExt;
use p2panda_core::traits::{Digest, OperationId};
use p2panda_core::{Extensions, Hash, LogId, Operation, PublicKey, SeqNum, Topic};
use p2panda_store::Transaction;
use p2panda_store::logs::LogStore;
use p2panda_store::operations::OperationStore;
use p2panda_store::topics::TopicStore;
use p2panda_stream::StreamLayerExt;
use p2panda_stream::ingest::{Ingest, IngestArguments};
use tokio::pin;
use tokio::runtime::Builder;
use tokio::sync::{Mutex, Notify, RwLock, mpsc, oneshot};
use tokio::task::LocalSet;
use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::processor::tasks::ProcessorTasks;

#[derive(Clone)]
pub struct Processor<L, E, TP> {
    pipeline_tx: mpsc::UnboundedSender<IngestArguments<L, E, TP>>,
    tasks: ProcessorTasks<Operation<E>, Hash>,
}

impl<L, E, TP> Processor<L, E, TP>
where
    L: LogId + Send + 'static,
    E: Extensions + Send + 'static,
    TP: Send + 'static,
{
    pub fn new<S>(store: S) -> Self
    where
        S: Transaction
            + OperationStore<Operation<E>, Hash, L>
            + LogStore<Operation<E>, PublicKey, L, SeqNum, Hash>
            + TopicStore<TP, PublicKey, L>
            + Send
            + 'static,
    {
        let (pipeline_tx, pipeline_rx) = mpsc::unbounded_channel();
        let tasks = ProcessorTasks::new();

        {
            let tasks = tasks.clone();
            // TODO: Handle error.
            let rt = Builder::new_current_thread().enable_all().build().unwrap();

            thread::spawn(move || {
                let local = LocalSet::new();

                local.spawn_local(async move {
                    // Prepare event processing pipeline.
                    let ingest = Ingest::<S, L, E, TP>::new(store);

                    // Receive incoming events through mpsc channel.
                    let pipeline = UnboundedReceiverStream::new(pipeline_rx)
                        // TODO: Later we want to add a prefix pruning processor here as well.
                        .layer(ingest)
                        .filter_map(|result| async {
                            // TODO: This is where we mark the operation as "done" or "failed" for the
                            // "ingest" processor and store that state in a new `ProcessorStore`.
                            //
                            // TODO: We don't want to filter out failed ingests here, see comment
                            // below.
                            result.ok()
                        });

                    pin!(pipeline);

                    while let Some(output) = pipeline.next().await {
                        tasks.mark_as_done(output.hash(), output).await;
                    }
                });

                rt.block_on(local);
            });
        }

        Self { pipeline_tx, tasks }
    }

    // TODO: We want to return errors of the processor here as well but for this we would need to
    // wrap the to-be-processed operation to hold additional information, like it's hash.
    //
    // For this to function we have to make the `Ingest` processor more generic. See:
    // https://github.com/p2panda/p2panda/issues/1038 and see comment in ingest codebase.
    pub async fn process(&self, input: IngestArguments<L, E, TP>) -> Operation<E> {
        // Queue up operation as a task so we can mark it as ready later.
        let task = self.tasks.queue(input.operation.hash()).await;

        // Send operation to processing pipeline, it will handle this operation eventually.
        let _ = self.pipeline_tx.send(input);

        // When it was marked as ready we continue here. Until then just block and wait. This
        // assures that operations are handled in-order.

        task.ready().await
    }
}

#[cfg(test)]
mod tests {
    use p2panda_core::Topic;
    use p2panda_core::test_utils::TestLog;
    use p2panda_store::SqliteStore;
    use p2panda_stream::ingest::IngestArguments;

    use super::Processor;

    #[tokio::test]
    async fn it_works() {
        let store = SqliteStore::temporary().await;
        let processor = Processor::<Topic, (), Topic>::new(store);

        let log = TestLog::new();
        let topic = Topic::new();

        let operation = log.operation(b"test", ());

        let result = processor
            .process(IngestArguments {
                operation: operation.clone(),
                log_id: topic,
                topic,
                prune_flag: false,
            })
            .await;

        assert_eq!(result, operation);
    }
}
