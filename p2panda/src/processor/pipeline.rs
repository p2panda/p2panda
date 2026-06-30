// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::VecDeque;
use std::fmt::Debug;
use std::sync::Arc;
use std::thread;

use futures_util::StreamExt;
use p2panda_core::traits::{Digest, ShortFormat};
use p2panda_core::{Extensions, Hash, LogId};
use p2panda_store::SqliteStore;
use p2panda_store::spaces::SqliteSpacesStore;
use p2panda_stream::StreamLayerExt;
use p2panda_stream::ingest::Ingest;
use p2panda_stream::log_prune::LogPrune;
use serde::{Deserialize, Serialize};
use tokio::pin;
use tokio::runtime::Builder;
use tokio::sync::{Mutex, Notify, mpsc};
use tokio::task::LocalSet;
use tokio_stream::wrappers::ReceiverStream;
use tracing::warn;

use crate::processor::orderer::Orderer;
use crate::processor::tasks::TaskTracker;
use crate::processor::{Event, ProcessorStatus};
use crate::spaces::types::{SpacesManager, SpacesProcessor};

/// Number of items which can stay in the pipeline input buffer before backpressure is applied.
///
/// If the buffer runs full, then sending of new operations into the processor will wait.
const TO_PIPELINE_BUFFER_SIZE: usize = 128;

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
///
/// ## Cloning pipelines
///
/// Re-using a pipeline across streams can lead to undesirable effects such as a) receiving
/// unwanted output events which were not intended for the topic stream b) broadcast channel designs
/// dropping events when running full.
/// ```
#[derive(Clone, Debug)]
pub struct Pipeline<L, E, TP> {
    to_pipeline_tx: mpsc::Sender<Event<L, E, TP>>,
    from_pipeline_queue: Arc<Mutex<VecDeque<Event<L, E, TP>>>>,
    from_pipeline_notify: Arc<Notify>,
    tasks: TaskTracker<Event<L, E, TP>, Hash>,
}

impl<L, E, TP> Pipeline<L, E, TP>
where
    // NOTE: Unfortunately there's no scoped "spawn_local" yet (it's an experimental tokio feature)
    // and we need to require a Send + 'static trait bounds, even though it's not used anywhere.
    L: LogId + Send + 'static,
    E: Extensions + Send + 'static,
    TP: Clone + Send + Serialize + for<'a> Deserialize<'a> + 'static,
{
    /// Creates a new "event processor" pipeline.
    ///
    /// Internally this spawns the whole pipeline inside a new thread with it's own tokio runtime.
    ///
    /// Users can run multiple pipelines parallely, a common task manager instance makes sure that
    /// processors do not work on the same event at the same time.
    pub fn new(
        store: SqliteStore,
        tasks: TaskTracker<Event<L, E, TP>, Hash>,
        spaces_manager: SpacesManager,
    ) -> Self {
        let (to_pipeline_tx, to_pipeline_rx) = mpsc::channel(TO_PIPELINE_BUFFER_SIZE);
        let from_pipeline_queue = Arc::new(Mutex::new(VecDeque::new()));
        let from_pipeline_notify = Arc::new(Notify::new());

        {
            let tasks = tasks.clone();

            let rt = Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("runtime for current thread");

            let from_pipeline_queue = from_pipeline_queue.clone();
            let from_pipeline_notify = from_pipeline_notify.clone();

            thread::spawn(move || {
                let local = LocalSet::new();

                local.spawn_local(async move {
                    // Prepare event processing pipeline.
                    let ingest =
                        Ingest::<SqliteStore, Event<L, E, TP>, L, E, TP>::new(store.clone());
                    let orderer = Orderer::<SqliteStore, Event<L, E, TP>, E>::new(store.clone());
                    let log_prune =
                        LogPrune::<SqliteStore, Event<L, E, TP>, L, E>::new(store.clone());
                    let spaces = SpacesProcessor::<Event<L, E, TP>>::new(
                        SqliteSpacesStore::new(store),
                        spaces_manager,
                    );

                    // Receive incoming events through mpsc channel.
                    let pipeline = ReceiverStream::new(to_pipeline_rx)
                        .layer(ingest)
                        .map(|result| match result {
                            Ok((mut event, result)) => {
                                event.ingest = ProcessorStatus::Completed(result);
                                event
                            }
                            Err((mut event, err)) => {
                                event.ingest = ProcessorStatus::Failed(err);
                                event.noop()
                            }
                        })
                        .layer(orderer)
                        .map(|result| match result {
                            Ok((mut event, result)) => {
                                event.orderer = ProcessorStatus::Completed(result);

                                // If the orderer returns a "pending" result we don't want to affect
                                // any next processors anymore.
                                if event.is_pending() {
                                    event.noop()
                                } else {
                                    event
                                }
                            }
                            Err((mut event, err)) => {
                                event.orderer = ProcessorStatus::Failed(err);
                                event.noop()
                            }
                        })
                        .layer(log_prune)
                        .map(|result| match result {
                            Ok((mut event, result)) => {
                                event.log_prune = ProcessorStatus::Completed(result);
                                event
                            }
                            Err((mut event, err)) => {
                                event.log_prune = ProcessorStatus::Failed(err);
                                event.noop()
                            }
                        })
                        .layer(spaces)
                        .map(|result| match result {
                            Ok((mut event, result)) => {
                                event.spaces = ProcessorStatus::Completed(result);
                                event
                            }
                            Err((mut event, err)) => {
                                event.spaces = ProcessorStatus::Failed(err);
                                event.noop()
                            }
                        });

                    pin!(pipeline);

                    while let Some(output_event) = pipeline.next().await {
                        if let Some(err) = output_event.failure_reason() {
                            warn!(
                                id = %output_event.hash().fmt_short(),
                                "failed processing event: {}",
                                err
                            );
                        }

                        // This informs any process waiting for the input event to be finished.
                        // Unknown tasks are ignored.
                        tasks
                            .mark_as_done(output_event.hash(), output_event.clone())
                            .await;

                        // If the output event is "ready" (that is, _not_ pending, not being
                        // buffered somewhere), then we can finally forward it on the output stream
                        // towards the application layer.
                        if !output_event.is_pending() {
                            from_pipeline_queue.lock().await.push_back(output_event);
                            from_pipeline_notify.notify_one(); // Wake up any pending next call
                        }
                    }
                });

                rt.block_on(local);
            });
        }

        Self {
            to_pipeline_tx,
            from_pipeline_queue,
            from_pipeline_notify,
            tasks,
        }
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
        let _ = self.to_pipeline_tx.send(input).await;

        // Block and await here until the mananger received the signal that the task has finished.
        // This assures that operations are handled in-order.
        //
        // Please note that the task might have finished successfully or with a processor failure,
        // we do not treat the error here on this level.
        task.ready().await
    }

    pub async fn next(&mut self) -> Event<L, E, TP> {
        loop {
            if let Some(output) = self.from_pipeline_queue.lock().await.pop_front() {
                return output;
            }

            // Wait for notification that an item was added.
            self.from_pipeline_notify.notified().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches;
    use std::collections::HashSet;

    use p2panda_core::test_utils::{TestLog, setup_logging};
    use p2panda_core::traits::Digest;
    use p2panda_core::{PruneFlag, SigningKey, Topic};
    use p2panda_store::SqliteStore;

    use crate::credentials::Credentials;
    use crate::forge::OperationForge;
    use crate::operation::LogId;
    use crate::processor::orderer::{OrdererArgs, OrdererResult};
    use crate::processor::{ProcessorStatus, TaskTracker};
    use crate::spaces::spaces_manager;
    use crate::streams::Source;

    use super::{Event, Pipeline};

    #[tokio::test]
    async fn processing_operations() {
        setup_logging();

        let store = SqliteStore::temporary().await;
        let tasks = TaskTracker::new();
        let credentials = Credentials::generate();
        let forge = OperationForge::new(credentials.clone(), store.clone());
        let spaces_manager = spaces_manager(forge, credentials, store.clone())
            .await
            .unwrap();

        let processor = Pipeline::<LogId, (), Topic>::new(store, tasks, spaces_manager);

        let log = TestLog::new();
        let topic = Topic::random();

        let mut operation = log.operation(b"test", ());

        // Expect operation to be processed successfully.
        let result = processor
            .process(Event::new(
                operation.clone(),
                Source::LocalStore,
                LogId::from_topic(topic),
                topic,
                PruneFlag::default(),
                None,
            ))
            .await;

        assert_eq!(result.hash(), operation.hash());
        assert!(result.is_completed());
        assert!(!result.is_failed());

        // Replace public key of operation to make it invalid. We expect the processor to fail.
        operation.header.verifying_key = SigningKey::generate().verifying_key();

        let result = processor
            .process(Event::new(
                operation.clone(),
                Source::LocalStore,
                LogId::from_topic(topic),
                topic,
                PruneFlag::default(),
                None,
            ))
            .await;

        assert_eq!(result.hash(), operation.hash());
        assert!(!result.is_completed());
        assert!(result.is_failed());
    }

    #[tokio::test]
    async fn out_of_order() {
        setup_logging();

        let store = SqliteStore::temporary().await;
        let tasks = TaskTracker::new();
        let credentials = Credentials::generate();
        let forge = OperationForge::new(credentials.clone(), store.clone());
        let spaces_manager = spaces_manager(forge, credentials, store.clone())
            .await
            .unwrap();

        let processor = Pipeline::<LogId, (), Topic>::new(store, tasks, spaces_manager);

        let mut events = Vec::new();
        let mut dependencies = Vec::new();

        // Create many operations in own logs (each depth 1) which are dependent on each other
        // (multi-writer). We reverse the order of how they are processed afterwards, so we need to
        // process everything in "the worst order possible".
        let topic = Topic::random();
        for _ in 0..255 {
            let log = TestLog::new();
            let operation = log.operation(b"op", ());

            let mut event = Event::new(
                operation.clone(),
                Source::LocalStore,
                LogId::from_topic(topic),
                topic,
                PruneFlag::default(),
                None,
            );

            event.orderer_args = OrdererArgs::Process {
                dependencies: dependencies,
            };

            events.push(event);

            dependencies = vec![operation.hash()];
        }

        events.reverse();

        for event in events {
            let event_hash = event.hash();
            let result = processor.process(event).await;

            assert_eq!(result.hash(), event_hash);
            assert!(result.is_completed());
            assert!(!result.is_failed());
        }
    }

    #[tokio::test]
    async fn buffered_outputs() {
        setup_logging();

        let store = SqliteStore::temporary().await;
        let tasks = TaskTracker::new();
        let credentials = Credentials::generate();
        let forge = OperationForge::new(credentials.clone(), store.clone());
        let spaces_manager = spaces_manager(forge, credentials, store.clone())
            .await
            .unwrap();

        let mut pipeline = Pipeline::<LogId, (), Topic>::new(store, tasks, spaces_manager);

        let log_icebear = TestLog::new();
        let log_panda = TestLog::new();
        let log_penguin = TestLog::new();

        let topic = Topic::random();

        let event_1 = {
            let mut event = Event::new(
                log_icebear.operation(b"op", ()),
                Source::LocalStore,
                LogId::from_topic(topic),
                topic,
                PruneFlag::default(),
                None,
            );
            event.orderer_args = OrdererArgs::Process {
                dependencies: vec![],
            };
            event
        };

        let event_2 = {
            let mut event = Event::new(
                log_panda.operation(b".. or no-op", ()),
                Source::LocalStore,
                LogId::from_topic(topic),
                topic,
                PruneFlag::default(),
                None,
            );
            event.orderer_args = OrdererArgs::Process {
                dependencies: vec![event_1.hash()],
            };
            event
        };

        let event_3 = {
            let mut event = Event::new(
                log_penguin.operation(b"that's the question", ()),
                Source::LocalStore,
                LogId::from_topic(topic),
                topic,
                PruneFlag::default(),
                None,
            );
            event.orderer_args = OrdererArgs::Process {
                dependencies: vec![event_1.hash()],
            };
            event
        };

        // 3 and 2 depend on 1. We send event 1 at the very end and expect 2 and 3 to be freed
        // "at the same time":
        //
        // [3]
        //     \
        //      -> [1]
        //     /
        // [2]

        let events = [event_3.clone(), event_2.clone(), event_1.clone()];

        // Input all three events, here we just expect processing to finish.
        for event in events {
            let event_hash = event.hash();
            let result = pipeline.process(event).await;

            assert_eq!(result.hash(), event_hash);
            assert!(result.is_completed());
            assert!(!result.is_failed());
        }

        // When calling "next" on the pipeline we expect the 3 events to come out:
        let result = pipeline.next().await;
        assert!(!result.is_failed());
        assert_eq!(result.hash(), event_1.hash());

        if let ProcessorStatus::Completed(OrdererResult::Resolved {
            mut dependent_operations,
        }) = result.orderer
        {
            let mut expected_operations = vec![event_2.hash(), event_3.hash()];

            expected_operations.sort();
            dependent_operations.sort();

            assert_eq!(expected_operations, dependent_operations);
        } else {
            panic!("unexpected orderer result");
        }

        // 2 or 3 can arrive in any order.
        let mut expected_hashes: HashSet<p2panda_core::Hash> =
            HashSet::from_iter([event_2.hash(), event_3.hash()]);

        let result = pipeline.next().await;
        assert!(expected_hashes.remove(&result.hash()));
        assert!(!result.is_failed());
        assert_matches!(
            result.orderer,
            ProcessorStatus::Completed(OrdererResult::Ready)
        );

        let result = pipeline.next().await;
        assert!(!result.is_failed());
        assert!(expected_hashes.remove(&result.hash()));
        assert_matches!(
            result.orderer,
            ProcessorStatus::Completed(OrdererResult::Ready)
        );

        assert_eq!(expected_hashes.len(), 0);
    }
}
