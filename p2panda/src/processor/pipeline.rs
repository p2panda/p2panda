// SPDX-License-Identifier: MIT OR Apache-2.0

use std::thread;

use futures_util::StreamExt;
use p2panda_core::traits::Digest;
use p2panda_core::{Extensions, Hash, LogId, Operation, PublicKey, SeqNum};
use p2panda_store::Transaction;
use p2panda_store::logs::LogStore;
use p2panda_store::operations::OperationStore;
use p2panda_store::topics::TopicStore;
use p2panda_stream::StreamLayerExt;
use p2panda_stream::ingest::Ingest;
use tokio::pin;
use tokio::runtime::Builder;
use tokio::sync::mpsc;
use tokio::task::LocalSet;
use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::processor::tasks::TaskTracker;
use crate::processor::{Event, ProcessorStatus};

/// Event processor pipeline which consists of multiple processors.
///
/// ```text
///           Event
///             |
///             |
///   Pipeline  v
///   +-------------------+
///   | Processor         |
///   | +---------------+ |
///   | |               | |
///   | |      ...      | |
///   | |               | |
///   | +-------+-------+ |
///   |         v         |
///   | Processor         |
///   | +---------------+ |
///   | |               | |
///   | |      ...      | |
///   | |               | |
///   | +-------+-------+ |
///   |         |         |
///   +---------+---------+
///             |
///             v
///           Event
/// ```
#[derive(Clone)]
pub struct Pipeline<L, E, TP> {
    pipeline_tx: mpsc::UnboundedSender<Event<L, E, TP>>,
    tasks: TaskTracker<Event<L, E, TP>, Hash>,
}

impl<L, E, TP> Pipeline<L, E, TP>
where
    // NOTE: Unfortunately there's no scoped "spawn_local" yet (it's an experimental tokio feature)
    // and we need to require a Send + 'static trait bounds, even though it's not used anywhere.
    L: LogId + Send + 'static,
    E: Extensions + Send + 'static,
    TP: Clone + Send + 'static,
{
    /// Creates a new "event processor" pipeline.
    ///
    /// Internally this spawns the whole pipeline inside a new thread with it's own tokio runtime.
    ///
    /// Users can run multiple pipelines parallely, a common task manager instance makes sure that
    /// processors do not work on the same event at the same time.
    //
    // NOTE: For now this creates a simple pipeline, in the future we might want different
    // pipelines for different streams (one with almost no processing and others with more complex
    // processing required for p2panda-spaces, etc.).
    //
    // NOTE: For parallelizing pipelines some sort of "work stealing" approach will be required.
    pub fn new<S>(store: S, tasks: TaskTracker<Event<L, E, TP>, Hash>) -> Self
    where
        S: Transaction
            + OperationStore<Operation<E>, Hash, L>
            + LogStore<Operation<E>, PublicKey, L, SeqNum, Hash>
            + TopicStore<TP, PublicKey, L>
            + Send
            + 'static,
    {
        let (pipeline_tx, pipeline_rx) = mpsc::unbounded_channel();

        {
            let tasks = tasks.clone();

            let rt = Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("runtime for current thread");

            thread::spawn(move || {
                let local = LocalSet::new();

                local.spawn_local(async move {
                    // Prepare event processing pipeline.
                    let ingest = Ingest::<S, Event<L, E, TP>, L, E, TP>::new(store);

                    // Receive incoming events through mpsc channel.
                    let pipeline =
                        UnboundedReceiverStream::new(pipeline_rx)
                            .layer(ingest)
                            .map(|result| match result {
                                Ok(mut operation) => {
                                    operation.ingest = ProcessorStatus::Completed(());
                                    operation
                                }
                                Err((mut operation, err)) => {
                                    operation.ingest = ProcessorStatus::Failed(err);
                                    operation
                                }
                            });

                    pin!(pipeline);

                    while let Some(operation) = pipeline.next().await {
                        tasks.mark_as_done(operation.hash(), operation).await;
                    }
                });

                rt.block_on(local);
            });
        }

        Self { pipeline_tx, tasks }
    }

    /// Queue up an incoming operation to be processed by this pipeline in the background.
    ///
    /// ## Strict ordering
    ///
    /// This method awaits until the pipeline finished this operation, assuring that incoming
    /// events stay in the same order as before. If this happens to be a bottleneck ("head-of-line
    /// blocking"), work can be parallelised by utilising multiple pipelines.
    ///
    /// ## Error handling
    ///
    /// This method does not return an error if a processor failed but instead users will need to
    /// check on the returned object itself if an error was observed.
    pub async fn process(&self, input: Event<L, E, TP>) -> Event<L, E, TP> {
        // Register task for this operation so the processor can mark it as *ready* later.
        let task = self.tasks.track(input.hash()).await;

        // Send operation to processing pipeline, it will handle this operation eventually in a
        // concurrent "background" task.
        let _ = self.pipeline_tx.send(input);

        // Block and await here until the mananger received the signal that the task has finished.
        // This assures that operations are handled in-order.
        //
        // Please note that the task might have finished successfully or with a processor failure,
        // we do not treat the error here on this level.
        task.ready().await
    }
}

#[cfg(test)]
mod tests {
    use p2panda_core::test_utils::TestLog;
    use p2panda_core::traits::Digest;
    use p2panda_core::{PrivateKey, Topic};
    use p2panda_store::SqliteStore;

    use crate::processor::TaskTracker;

    use super::{Event, Pipeline};

    #[tokio::test]
    async fn processing_operations() {
        let store = SqliteStore::temporary().await;
        let tasks = TaskTracker::new();
        let processor = Pipeline::<Topic, (), Topic>::new(store, tasks);

        let log = TestLog::new();
        let topic = Topic::new();

        let mut operation = log.operation(b"test", ());

        // Expect operation to be processed successfully.
        let result = processor
            .process(Event::new(operation.clone(), topic, topic))
            .await;

        assert_eq!(result.hash(), operation.hash());
        assert!(result.is_completed());
        assert!(!result.is_failed());

        // Replace public key of operation to make it invalid. We expect the processor to fail.
        operation.header.public_key = PrivateKey::new().public_key();

        let result = processor
            .process(Event::new(operation.clone(), topic, topic))
            .await;

        assert_eq!(result.hash(), operation.hash());
        assert!(!result.is_completed());
        assert!(result.is_failed());
    }
}
