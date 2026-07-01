// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashSet, VecDeque};
use std::fmt::Debug;
use std::hash::Hash as StdHash;
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

use crate::processor::orderer::{Orderer, OrdererResult};
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
/// ## Important design considerations
///
/// ### Task tracking
///
/// Every input event needs to _strictly_ lead to an equivalent output event (1:1), otherwise the
/// process who inserted the event will await a result forever. This is managed by the task tracker.
///
/// For processors who buffer events (because they are out-of-order) we need to output "pending"
/// events to inform the task tracker that the event was successfully processed and is now buffered
/// for a while.
///
/// ### Cloning pipelines
///
/// Re-using a pipeline across streams can lead to undesirable effects such as receiving unwanted
/// output events which were not intended for the topic stream.
///
/// Only clone the pipeline _within_ a topic stream. Across different topic streams a new pipeline
/// should be created.
///
/// ### Failed or pending events
///
/// Any failed or pending event needs to strictly be "disabled" for any processor which follows
/// after being marked as such. A failed event can be due to an unauthorized action which should not
/// have any further effect. A pending event is "out of order" and should not have any effect _yet_
/// until it is "ready" / in-order.
///
/// The event needs to still travel through the pipeline, to be successfully marked as "done" (see
/// "Task tracking") but shouldn't cause any effects in processors anymore. This is achieved by
/// calling the `noop` method on the event. It will set all processor arguments to "ignore".
///
/// ### Input / Output separation and ordering
///
/// Input event streams are independent from the output event stream. Both streams are supposed to
/// be ordered, that is:
///
/// 1. Input events should be marked as done _after_ all caused side effects have been processed,
///    the order on how they were input needs to be preserved, independent of buffering
/// 2. Output events should arrive in topologically sorted, causal order (if required)
///
/// Both rules are important for the whole system to function correctly and not cause surprising
/// results to the application layer.
///
/// Pending events are not forwarded to the output stream.
///
/// For upholding all orderer rules the following takes place, here shown with a simple example:
///
/// ```text
/// A is dependent on B:
/// [A] -> [B]
///
/// 1. A is inserted _before_ B into the pipeline:
///
/// [A] <- marked as "pending" since out-of-order
/// [B] <- marked as "ready input", since in-order & freeing A
///
/// 2. On the *inner* output stream we receive now:
///
/// [A] "pending" <- not forwarded to outer output stream *
/// [B] "ready input" *
/// [A] "ready output"
///
/// *) These events were part of the input stream.
///
/// 3. On the *outer* output stream we receive now:
///
/// [B]
/// [A]
///
/// 4. We wait until A was put on the output stream to mark B as done:
///
/// [A] <- .. is on the output stream now (step 3.)
/// [B] <- We mark this input task as "done" now
///
/// Note how we've changed the ordering of A and B in Step 2. and 3. but needed to preserve the
/// original input ordering in 4.
/// ```
///
/// The topological ordering is assured by the "orderer" processor. In step 3. we can see how B is
/// put on the output stream _before_ A even though they were input in reversed order.
///
/// To ensure that B is still marked _after_ all side-effects (freed A) took place we keep track of
/// the dependents of B. When they all left the output stream we can finally mark B as done.
///
/// This ensures that any process awaiting the result of processing B will be marked ready in the
/// original input order.
//
// FIXME: The pipeline thread keeps currently running even when the struct was dropped.
// See related issue: https://github.com/p2panda/p2panda/issues/1275
#[derive(Clone, Debug)]
pub struct Pipeline<L, E, TP> {
    id: Hash,
    to_pipeline_tx: mpsc::Sender<Event<L, E, TP>>,
    from_pipeline_queue: Arc<Mutex<VecDeque<Event<L, E, TP>>>>,
    from_pipeline_notify: Arc<Notify>,
    tasks: TaskTracker<Event<L, E, TP>, PipelineTaskId>,
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
        id: impl Into<Hash>,
        store: SqliteStore,
        tasks: TaskTracker<Event<L, E, TP>, PipelineTaskId>,
        spaces_manager: SpacesManager,
    ) -> Self {
        let pipeline_id = id.into();

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
                    let me = spaces_manager.id();
                    // Prepare event processing pipeline.
                    let ingest =
                        Ingest::<SqliteStore, Event<L, E, TP>, L, E, TP>::new(store.clone());
                    let orderer = Orderer::<SqliteStore, Event<L, E, TP>, E>::new(store.clone());
                    let log_prune =
                        LogPrune::<SqliteStore, Event<L, E, TP>, L, E>::new(store.clone());

                    let spaces_store = SqliteSpacesStore::new(store);
                    let spaces = SpacesProcessor::<Event<L, E, TP>>::new(
                        spaces_store.clone(),
                        spaces_manager.clone(),
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

                    let mut pending_dependencies = HashSet::<Hash>::new();
                    let mut pending: Option<Event<L, E, TP>> = None;

                    while let Some(output_event) = pipeline.next().await {
                        // TODO: We need to handle "invalidating" the pending buffers when something
                        // went wrong _after_ the orderer processor. Otherwise these items might be
                        // stuck here forever.
                        if let Some(err) = output_event.failure_reason() {
                            warn!(
                                me = me.fmt_short(),
                                id = %output_event.hash().fmt_short(),
                                "failed processing event: {}",
                                err
                            );
                        }

                        // An event arrived which "freed" some dependent operations. This "parent"
                        // event needs to be forwarded _first_ on the output, but we want it to be
                        // processed _last_ on the input side. See documentation above for more
                        // context.
                        //
                        // For this we are "reversing" the dependency tree and delay marking the
                        // "parent" event as done when all dependencies have been visited.
                        if let ProcessorStatus::Completed(OrdererResult::ReadyInput {
                            ref dependent_operations,
                        }) = output_event.orderer
                        {
                            pending_dependencies = HashSet::from_iter(dependent_operations.clone());
                            pending = Some(output_event.clone());
                        }

                        // We've visited a dependency.
                        if let ProcessorStatus::Completed(OrdererResult::ReadyOutput) =
                            output_event.orderer
                        {
                            pending_dependencies.remove(&output_event.hash());
                        }

                        // If the output event is "ready" (that is, _not_ pending / not being
                        // buffered somewhere), then we can finally forward it on the output stream
                        // towards the application layer.
                        //
                        // We want this to happen _before_ we mark the task as done to allow all
                        // consumers to yield the items in correct order.
                        if !output_event.is_pending() {
                            from_pipeline_queue
                                .lock()
                                .await
                                .push_back(output_event.clone());
                            from_pipeline_notify.notify_one(); // Wake up any pending next call
                        }

                        // This informs any process waiting for the input event to be finished.
                        if pending.is_some() && pending_dependencies.is_empty() {
                            // All dependencies have been visited, we can finally mark the "parent"
                            // input event  as done.
                            if let Some(pending) = pending.take() {
                                let task_id = PipelineTaskId {
                                    event_id: pending.hash(),
                                    pipeline_id,
                                };

                                tasks.mark_as_done(task_id, pending).await;
                            }
                        } else {
                            let task_id = PipelineTaskId {
                                event_id: output_event.hash(),
                                pipeline_id,
                            };

                            tasks.mark_as_done(task_id, output_event).await;
                        }
                    }
                });

                rt.block_on(local);
            });
        }

        Self {
            id: pipeline_id,
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
        let task_id = PipelineTaskId {
            event_id: input.hash(),
            pipeline_id: self.id,
        };

        // Register task for this operation so the processor can mark it as *ready* later.
        let task = self.tasks.track(task_id).await;

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

/// Identifier for a single task in the task tracker used by the pipeline.
///
/// Every task is grouped by the pipeline itself and then the regarding event id. Like this we can
/// make sure that there will be no collisions across pipelines (different topic streams but same
/// operations).
///
/// This is deliberately not using a hashing function to safe computing cost.
#[derive(Copy, Clone, Debug, PartialEq, Eq, StdHash)]
pub struct PipelineTaskId {
    pipeline_id: Hash,
    event_id: Hash,
}

#[cfg(test)]
mod tests {
    use std::assert_matches;
    use std::collections::HashSet;

    use p2panda_core::test_utils::{TestLog, setup_logging};
    use p2panda_core::traits::Digest;
    use p2panda_core::{Hash, PruneFlag, SigningKey, Topic};
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

        let pipeline_id = Hash::from([0; 32]);
        let pipeline = Pipeline::<LogId, (), Topic>::new(pipeline_id, store, tasks, spaces_manager);

        let log = TestLog::new();
        let topic = Topic::random();

        let mut operation = log.operation(b"test", ());

        // Expect operation to be processed successfully.
        let result = pipeline
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

        let result = pipeline
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

        let pipeline_id = Hash::from([0; 32]);
        let pipeline = Pipeline::<LogId, (), Topic>::new(pipeline_id, store, tasks, spaces_manager);

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
            let result = pipeline.process(event).await;

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

        let pipeline_id = Hash::from([0; 32]);
        let mut pipeline =
            Pipeline::<LogId, (), Topic>::new(pipeline_id, store, tasks, spaces_manager);

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

        if let ProcessorStatus::Completed(OrdererResult::ReadyInput {
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
            ProcessorStatus::Completed(OrdererResult::ReadyOutput)
        );

        let result = pipeline.next().await;
        assert!(!result.is_failed());
        assert!(expected_hashes.remove(&result.hash()));
        assert_matches!(
            result.orderer,
            ProcessorStatus::Completed(OrdererResult::ReadyOutput)
        );

        assert_eq!(expected_hashes.len(), 0);
    }
}
