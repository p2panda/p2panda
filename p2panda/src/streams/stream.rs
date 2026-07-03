// SPDX-License-Identifier: MIT OR Apache-2.0

use std::borrow::Borrow;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_util::stream::BoxStream;
use futures_util::{FutureExt, Stream, StreamExt};
use p2panda_core::cbor::{DecodeError, EncodeError, decode_cbor, encode_cbor};
use p2panda_core::traits::Digest;
use p2panda_core::{Hash, Topic, VerifyingKey};
use p2panda_net::NodeId;
use p2panda_net::sync::SyncHandle;
// TODO: Replace with ShortFormat from p2panda-core.
// See: https://github.com/p2panda/p2panda/issues/1270
use p2panda_net::utils::ShortFormat;
use p2panda_spaces::{GroupEvent, SpaceEvent};
use p2panda_store::SqliteStore;
use p2panda_store::operations::OperationStore;
use p2panda_stream::spaces::SpacesResult;
use p2panda_sync::protocols::TopicLogSyncEvent;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, warn};

use crate::forge::{Forge, ForgeError, OperationForge};
use crate::node::{AckPolicy, CreateStreamError};
use crate::operation::{Extensions, Header, LogId, Operation};
use crate::processor::{ProcessorError, ProcessorStatus};
use crate::spaces::types::{AuthCapabilities, SpacesManager};
use crate::spaces::{RepairError, spawn_repair_task};
use crate::streams::acked::{Acked, AckedError};
use crate::streams::external_stream::{
    ExternalStream, ExternalStreamEvent, ExternalStreamFuture, SessionId,
};
use crate::streams::replay::{ReplayError, StreamFrom, replay_log_ranges};
use crate::streams::sync_metrics::{self, Aggregator, SessionPhase, SyncError};
use crate::streams::{Event, Pipeline};

/// Number of items which can stay in the buffer before the application-layer picks up the
/// operations. If buffer runs full the processor will pause work and we'll apply backpressure to
/// the sync backend.
const BUFFER_SIZE: usize = 16;

/// Number of items which can stay in the buffer before processing kicks in for locally published
/// items. If buffer runs full, creating new operations will apply backpressure.
const PUBLISH_BUFFER_SIZE: usize = 128;

/// Number of streams which can stay in the import buffer. If the buffer runs full importing of
/// new streams will have backpressure applied.
const IMPORT_BUFFER_SIZE: usize = 16;

/// Returns publish and subscribe halfs of an eventually consistent messaging stream for a given
/// topic.
///
/// ## At-least-once guarantee
///
/// Rare race-conditions might occur where operations can arrive multiple times at the application
/// layer. This is why we're only providing an *at-least-once* guarantee.
///
/// It is recommended to either add measures to provide an *exactly-once* guarantee or make sure
/// all application logic is idempotent.
///
/// ## Stream design
///
/// Locally published operations are processed by the same event processor pipeline as incoming,
/// remote operations.
///
/// If a replay was requested, processing local and remotely incoming operations is temporarily
/// blocked until replay has finished.
///
/// ```text
/// ┌────────────────┐                ┌─────────┐
/// │ LogSync Stream │                │ Publish │
/// └──────────────┬─┘                └─┬───────┘
///                │                    │
///                /  Replay can block  /
///                │                    │
///                │  ┌──────────────┐  │
///                │  │    Replay    │  │
///                │  └──────────────┘  │
///                │         │          │
///                │         │          │
///                │  ┌──────▼───────┐  │
///                └──►              ◄──┘
///                   │   Pipeline   │
///                   │              │
///                   └──────┬───────┘
///                          │
///                          │
///                          │
///               ┌──────────▼──────────┐
///               │ Application Stream  │
///               └─────────────────────┘
/// ```
#[allow(clippy::too_many_arguments)]
pub(crate) async fn processed_stream<M>(
    topic: Topic,
    ack_policy: AckPolicy,
    sync_handle: SyncHandle<Operation, TopicLogSyncEvent<Extensions>>,
    store: SqliteStore,
    forge: OperationForge,
    spaces_manager: SpacesManager,
    pipeline: Pipeline,
    from: StreamFrom,
) -> Result<(StreamPublisher<M>, StreamSubscription<M>), CreateStreamError>
where
    M: Serialize + for<'a> Deserialize<'a> + Send + 'static,
{
    let acked = Acked::new(store.clone(), topic);

    // Sync handle is used on the publisher and when importing from external streams.
    let sync_handle = Arc::new(sync_handle);

    let mut sync_stream = sync_handle
        .subscribe()
        .await
        .map_err(|err| CreateStreamError(err.to_string()))?;

    // Channel to send processed events to the application-layer.
    let (app_tx, app_rx) = mpsc::channel::<StreamEvent<M>>(BUFFER_SIZE);

    // Channel to send locally created operations to the processing pipeline. A "oneshot" callback
    // is attached to allow publishers to await the processing result.
    let (publish_tx, mut publish_rx) =
        mpsc::channel::<(Operation, Option<M>, oneshot::Sender<Event>)>(PUBLISH_BUFFER_SIZE);

    // Channel for importing external operation streams.
    let (import_tx, mut import_rx) = mpsc::channel::<(
        BoxStream<'static, Operation>,
        oneshot::Sender<ExternalStreamFuture>,
    )>(IMPORT_BUFFER_SIZE);

    // Set of currently active external streams.
    let mut external_stream = ExternalStream::default();

    // Determine from which point on we re-play locally stored operations.
    let nacked_log_ranges = acked
        .nacked_log_ranges(from)
        .await
        .map_err(|err| CreateStreamError(err.to_string()))?;

    // If any other process wants to bring an stream event forward to the application layer ("output
    // stream"), this channel should be used.
    let (to_output_tx, mut to_output_rx) = mpsc::channel::<Vec<StreamEvent<M>>>(128);

    let (repair_tx, repair_rx) = mpsc::channel(1);

    // Task concerned with repairing a space.
    let repair_task_handle = spawn_repair_task(
        topic,
        spaces_manager.clone(),
        store.clone(),
        import_tx.clone(),
        repair_rx,
    );

    // FIXME: Both tasks need to be gracefully shut down when the tx / rx halves got dropped,
    // otherwise the sync session keeps running.
    //
    // See related issue: https://github.com/p2panda/p2panda/issues/1272

    // Spawn first task which receives processed "output events" from the processing pipeline, the
    // result is handled (acking, decoding, conversion to `StreamEvent`, etc.) and then finally
    // forwarded to the application layer.
    {
        let mut pipeline = pipeline.clone();
        let acked = acked.clone();

        tokio::spawn(async move {
            loop {
                let stream_events = tokio::select! {
                    // We need to process pipeline output events _before_ any other system events.
                    // This is crucial to ensure correct ordering of events such as "replay started"
                    // being followed by "processed operations" and then finally by "replay ended",
                    // etc.
                    biased;

                    // Handle resulting output events from the pipeline and forward them as stream
                    // events to application layer, when applicable.
                    from_pipeline_event = pipeline.next() => {
                        if let Some(stream_events) =
                            process_operation_out::<M>(
                                from_pipeline_event,
                                topic,
                                ack_policy,
                                &acked
                            ).await
                        {
                            stream_events
                        } else {
                            continue;
                        }
                    },

                    // Any other process forwarding an event (like "replay ended", etc.) to the
                    // application layer.
                    Some(stream_events) = to_output_rx.recv() => {
                        stream_events
                    }
                };

                // Send processing result to application layer.
                //
                // If channel stopped working because the subscriber got dropped, ignore it as
                // we still might want to process locally published operations.
                for stream_event in stream_events {
                    let _ = app_tx.send(stream_event).await;
                }
            }
        });
    }

    // Spawn second task which assembles different sources of incoming operations (replays, external
    // streams, sync session, locally published, etc.) and inputs them into the processing pipeline.
    //
    // Here we only forward "system events" to the output stream, such as "sync started" or
    // "external stream import finished", etc.
    {
        let store = store.clone();
        let sync_handle = sync_handle.clone();

        tokio::spawn(async move {
            // =======================
            // 1. Re-play local events
            // =======================

            {
                // This will block processing of the sync stream and of locally created operations
                // until it is complete.
                let replay_result =
                    replay_log_ranges(topic, &store, &to_output_tx, &pipeline, nacked_log_ranges)
                        .await;

                // Errors occurring in the replay task which be returned to the user.
                if let Err(error) = replay_result {
                    warn!(
                        topic = %topic.fmt_short(),
                        "error occurred in replay task: {error}"
                    );

                    let _ = to_output_tx
                        .send(vec![StreamEvent::ReplayFailed {
                            error: Arc::new(error),
                        }])
                        .await;
                }
            }

            // =========================
            // 2. Handle incoming events
            // =========================

            let mut aggregator = Aggregator::new();
            loop {
                let stream_events = tokio::select! {
                    // Received incoming operation from remote source.
                    item = sync_stream.next() => {
                        let Some(result) = item else {
                            // Log sync stream seized, we stop the task as well.
                            break;
                        };

                        // Ignore internal broadcast channel error, this only indicates that the
                        // channel dropped a message which we can't do much about on this layer
                        // anymore. In the future we want to remove this error type altogether.
                        //
                        // Related issue: https://github.com/p2panda/p2panda/issues/959
                        let Ok(from_sync) = result else {
                            continue;
                        };

                        let Some(event) = aggregator.process(from_sync) else {
                            continue;
                        };

                        match event {
                            sync_metrics::SyncEvent::SyncStarted { .. } => vec![event.into()],
                            sync_metrics::SyncEvent::SyncEnded { .. } => vec![event.into()],
                            sync_metrics::SyncEvent::OperationReceived { operation, source } => {
                                process_operation_in(*operation, source, topic, &pipeline).await;
                                continue;
                            },
                        }
                    }

                    // Received locally created operation which needs to be processed as well.
                    //
                    // If the publishing channel gets closed, for example when the publisher handle
                    // got dropped, we still continue with this task, as we still might receive
                    // operations from the log sync stream.
                    //
                    // NOTE: Spaces messages do _not_ come through this route; they all come
                    // through import (even ones published locally).
                    Some((operation, _message, processed_tx)) = publish_rx.recv() => {
                        let event = process_operation_in(
                            operation,
                            Source::LocalStore,
                            topic,
                            &pipeline,
                        ).await;

                        // Inform publisher optionally about result of processor and that we're
                        // done here.
                        let _ = processed_tx.send(event.clone());

                        continue;
                    }

                    // Receive imported external source of operations.
                    Some((stream, ready_tx)) = import_rx.recv() => {
                        let external_stream_future = external_stream.insert(stream);
                        let session_id = external_stream_future.session_id();
                        if ready_tx.send(external_stream_future).is_err() {
                            warn!(session_id = session_id, "failed sending on import ready channel")
                        };
                        continue;
                    }

                    // Receive the next ready event from any imported external source.
                    Some(event) = external_stream.next() => {
                        match event {
                            ExternalStreamEvent::Start {
                                session_id
                            } => vec![StreamEvent::ImportStarted { session_id }],
                            ExternalStreamEvent::Operation { session_id, operation } => {
                                // Try pushing operation to other nodes if we have an active and
                                // "live" sync session with them. This allows disseminating new
                                // messages quickly in the network.
                                //
                                // If no active live session exists, nodes will pick up the
                                // operation later when running the sync protocol.
                                //
                                // TODO: There are two paths for operations to be published into
                                // live-mode channels now (here and when one directly publishes new
                                // messages). Consider if these should be unified into one path
                                // somehow so as to avoid the chance of introducing inconsistency
                                // later on.
                                if sync_handle
                                    .publish(*operation.clone())
                                    .await.is_err() {
                                        warn!(
                                            session_id,
                                            operation_id = %operation.hash(),
                                            "failed sending imported operation on sync channel"
                                        )
                                    };

                                process_operation_in(
                                    *operation,
                                    Source::ExternalStream { session_id },
                                    topic,
                                    &pipeline,
                                ).await;

                                continue;
                            },
                            ExternalStreamEvent::End {
                                session_id
                            } => {
                                vec![StreamEvent::ImportEnded { session_id }]
                            },
                        }
                    },
                };

                let _ = to_output_tx.send(stream_events).await;
            }

            // Abort the repair task.
            repair_task_handle.abort();
        });
    }

    let tx = StreamPublisher {
        topic,
        sync_handle: sync_handle.clone(),
        forge,
        publish_tx,
        import_tx,
        repair_tx,
        _marker: PhantomData,
    };

    let rx = StreamSubscription {
        topic,
        store,
        sync_handle,
        acked,
        stream: ReceiverStream::new(app_rx),
    };

    Ok((tx, rx))
}

/// Process an incoming operation in the pipeline.
pub(crate) async fn process_operation_in(
    operation: Operation,
    source: Source,
    topic: Topic,
    pipeline: &Pipeline,
) -> Event {
    let log_id = operation.header.extensions.log_id();
    let prune_flag = operation.header.extensions.prune_flag();
    let spaces_args = operation.header.extensions.spaces_args();

    // Send operation to processor task. This blocks any parent stream and makes sure that all
    // events are handled in same order.
    let event = pipeline
        .process(Event::new(
            operation,
            source,
            log_id,
            topic,
            prune_flag,
            spaces_args,
        ))
        .await;

    // The actual output from the pipeline comes via a channel to the topic stream, see
    // process_operation_out. The returned event here is only for (optional) inspection for users
    // who published an operation locally and want to await until it is processed.
    event
}

/// Handle the resulting event output coming from the processor pipeline.
pub(crate) async fn process_operation_out<M>(
    event: Event,
    topic: Topic,
    ack_policy: AckPolicy,
    acked: &Acked,
) -> Option<Vec<StreamEvent<M>>>
where
    M: for<'a> Deserialize<'a> + Send + 'static,
{
    let source = event.source.clone();

    if let Some(error) = event.failure_reason() {
        warn!(
            id = %event.hash(),
            "processing operation failed: {}",
            error,
        );

        let failure_event = vec![StreamEvent::ProcessingFailed {
            event,
            error,
            source,
        }];

        return Some(failure_event);
    }

    // Collection of events to be returned from this function.
    //
    // Processing of a spaces event may yield multiple events to be forwarded to the user, while
    // processing any other event will only yield a single user event.
    let mut forward_events = Vec::new();

    // Process spaces events.
    if let ProcessorStatus::Completed(SpacesResult::Processed { ref events }) = event.spaces {
        // Multiple events can be released at once.
        for space_event in events {
            match space_event {
                p2panda_spaces::Event::Application { space_id: _, data } => {
                    match decode_cbor::<M, _>(&data[..]) {
                        Ok(message) => {
                            // Only ack events automatically if processing or decoding did not fail.
                            if ack_policy == AckPolicy::Automatic
                                && let Err(error) = acked.ack(&event).await
                            {
                                forward_events.push(StreamEvent::AckFailed {
                                    event: event.clone(),
                                    error: Arc::new(error),
                                });
                            }

                            forward_events.push(StreamEvent::Processed {
                                operation: ProcessedOperation {
                                    event: event.clone(),
                                    topic,
                                    acked: acked.clone(),
                                    message,
                                },
                                source: source.clone(),
                            });
                        }
                        Err(error) => forward_events.push(StreamEvent::DecodeFailed {
                            event: event.clone(),
                            error,
                        }),
                    }
                }

                p2panda_spaces::Event::KeyBundle { author } => {
                    forward_events.push(StreamEvent::KeyBundle(*author))
                }

                p2panda_spaces::Event::Group(group_event) => {
                    forward_events.push(StreamEvent::Group(group_event.to_owned()))
                }

                p2panda_spaces::Event::Space(space_event) => {
                    forward_events.push(StreamEvent::Space(space_event.to_owned()))
                }
            }
        }
    } else {
        // Only forward non-spaces operations to the application-layer if they have a body.
        let Some(body) = event.body() else {
            // _Always_ ack system-level events, even if no automatic policy was configured.
            if let Err(error) = acked.ack(&event).await {
                return Some(vec![StreamEvent::AckFailed {
                    event,
                    error: Arc::new(error),
                }]);
            }

            return None;
        };

        // Attempt decoding application-layer message. This takes place _after_ system-level
        // processing completed and the operation was ingested.
        //
        // In case decoding fails due to an application bug, users have the option to re-play this
        // persisted operation and attempt decoding again.
        //
        // If application data is malformed users can choose to remove the payload of the operation
        // or delete the whole log altogether.
        //
        // TODO: Is this mixing up concerns? We can only handle bytes on our end and let the users
        // do decoding on application layer?
        debug!(id = %event.operation.hash(), "processing application message");
        match decode_cbor::<M, _>(body.as_bytes()) {
            Ok(message) => {
                // Only ack events automatically if processing or decoding did not fail.
                if ack_policy == AckPolicy::Automatic
                    && let Err(error) = acked.ack(&event).await
                {
                    return Some(vec![StreamEvent::AckFailed {
                        event,
                        error: Arc::new(error),
                    }]);
                }

                return Some(vec![StreamEvent::Processed {
                    operation: ProcessedOperation {
                        event,
                        topic,
                        acked: acked.clone(),
                        message,
                    },
                    source,
                }]);
            }
            Err(error) => return Some(vec![StreamEvent::DecodeFailed { event, error }]),
        }
    }

    Some(forward_events)
}

/// Publish messages into a topic stream.
///
/// Any message type `M` can be published as long as it can be encoded into bytes by implementing
/// serde's [`Serialize`] and [`Deserialize`] traits.
///
/// ## Example
///
/// ```rust
/// # use p2panda_core::Topic;
/// # use serde::{Serialize, Deserialize};
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// # let node = p2panda::builder().spawn().await?;
/// #
/// let our_trees = Topic::random();
///
/// #[derive(Clone, Debug, Serialize, Deserialize)]
/// struct Tree {
///     leafy: bool,
///     latin_name: String,
/// }
///
/// let (tx, _rx) = node.stream::<Tree>(our_trees).await?;
///
/// tx.publish(Tree {
///     leafy: true,
///     latin_name: "Acer pseudoplatanus".into(),
/// }).await?;
/// #
/// # Ok(())
/// # }
/// ```
///
/// ## Append-only log
///
/// The Node API internally maintains an append-only log data-type for published application
/// messages in a topic stream.
///
/// Publishing a message creates and signs an [`Operation`] which gets automatically appended to
/// the author's log for the given topic. The message itself is the payload of the created
/// operation.
///
/// ```plain
/// Author "Panda"
/// Topic "Trees" Log: [ Header ] <-- [ Header ] <-- [ Header ] ...
///                         |             |              |
///                         v            ...            ...
///                     [ Body ]
///               "Acer pseudoplatanus"
///
/// Author "Icebear"
/// Topic "Trees" Log: [ Header ] <-- [ Header ] ...
///                         |             |
///                         v            ...
///                     [ Body ]
///                 "Pinus halepensis"
/// ```
///
/// ## External sources
///
/// Operations can be imported from external sources into the processing pipeline by calling
/// [`StreamPublisher::import`]. This allows transporting data via sneakernets (USB stick, etc.) or
/// other sync solutions.
#[derive(Clone, Debug)]
pub struct StreamPublisher<M> {
    topic: Topic,
    sync_handle: Arc<SyncHandle<Operation, TopicLogSyncEvent<Extensions>>>,
    forge: OperationForge,
    #[allow(clippy::type_complexity)]
    pub(crate) publish_tx: mpsc::Sender<(Operation, Option<M>, oneshot::Sender<Event>)>,
    import_tx: mpsc::Sender<(
        BoxStream<'static, Operation>,
        oneshot::Sender<ExternalStreamFuture>,
    )>,
    pub(crate) repair_tx: mpsc::Sender<oneshot::Sender<Result<bool, RepairError>>>,
    _marker: PhantomData<M>,
}

impl<M> StreamPublisher<M>
where
    M: Serialize,
{
    /// Associated topic.
    pub fn topic(&self) -> Topic {
        self.topic
    }

    /// Publish a message into a topic stream.
    ///
    /// Locally created operations are processed by the same pipeline as remotely received
    /// operations. It is possible to await the processing result which can be useful for some
    /// applications if they want to block UI components etc.
    pub async fn publish(&self, message: M) -> Result<PublishFuture, PublishError> {
        self.publish_inner(Some(message), false).await
    }

    /// Deletes all our previously published messages in this topic stream.
    ///
    /// This signals to all other nodes that they should remove them as well.
    ///
    /// A message can be optionally added when pruning, allowing to publish a "snapshot" /
    /// state-based CRDT of the current state, so nodes can still consistently re-create all state,
    /// even if previous messages are gone.
    ///
    /// Internally we're applying append-only log prefix deletion, meaning that the log's prefix
    /// gets pruned. The prefix is the set of operations in the log's sequence which are causally
    /// "older" / before the point where the prune flag was set.
    pub async fn prune(&self, message: Option<M>) -> Result<PublishFuture, PublishError> {
        self.publish_inner(message, true).await
    }

    /// Import an external source of operations.
    ///
    /// Please note: Operations do not contain any information by themselves about to which topic
    /// they belong. By importing operations into a topic stream, they will be assigned to this
    /// topic. Make sure you accordingly routed operations into the correct topic before.
    pub async fn import(
        &self,
        stream: impl Stream<Item = Operation> + Send + 'static,
    ) -> Result<ExternalStreamFuture, ImportError> {
        // Send stream to processor.
        let stream = Box::pin(stream);
        let (ready_tx, ready_rx) = oneshot::channel::<ExternalStreamFuture>();
        self.import_tx
            .send((stream, ready_tx))
            .await
            .map_err(|err| ImportError::SendToProcessor(err.to_string()))?;

        // Await receiving the session id and future which will complete when the external stream
        // closes and all operations have been processed.
        ready_rx
            .await
            .map_err(|err| ImportError::ReceiveFromProcessor(err.to_string()))
    }

    async fn publish_inner(
        &self,
        message: Option<M>,
        prune_flag: bool,
    ) -> Result<PublishFuture, PublishError> {
        // Create, sign and persist operation with given payload.
        let extensions = Extensions::builder(LogId::from_topic(self.topic()))
            .prune_flag(prune_flag)
            .build();

        let body_bytes = match message {
            Some(ref message) => Some(encode_cbor(&message)?),
            None => None,
        };

        let operation = self
            .forge
            .create_operation(
                Some(self.topic()),
                extensions.log_id(),
                body_bytes,
                extensions,
            )
            .await?;
        let hash = operation.hash;

        // Start processing operation in pipeline. Keep a oneshot receiver around to allow users to
        // optionally await & get informed when processing has finished.
        let (processed_tx, processed_rx) = oneshot::channel();
        self.publish_tx
            .send((operation.clone(), message, processed_tx))
            .await
            .map_err(|err| PublishError::SendToProcessor(err.to_string()))?;

        // Try pushing operation to other nodes if we have an active and "live" sync session with
        // them. This allows disseminating new messages fastly in the network.
        //
        // If no active live session exists, nodes will pick up the operation later when running the
        // sync protocol.
        self.sync_handle
            .publish(operation)
            .await
            .map_err(|err| PublishError::SyncHandle(err.to_string()))?;

        Ok(PublishFuture { hash, processed_rx })
    }
}

/// Future which can be awaited to find out when a locally published operation has finished
/// processing.
#[derive(Debug)]
pub struct PublishFuture {
    hash: Hash,
    processed_rx: oneshot::Receiver<Event>,
}

impl PublishFuture {
    /// Returns hash of the published operation.
    pub fn hash(&self) -> Hash {
        self.hash
    }
}

impl Future for PublishFuture {
    type Output = Result<Event, oneshot::error::RecvError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.processed_rx.poll_unpin(cx)
    }
}

/// Subscription to events arriving from a topic stream.
///
/// A topic stream emits:
///
/// - Locally created or remotely synced [`ProcessedOperation`] with application messages inside
/// - Topic-scoped system-events, for example if a sync session has begun and how much will be sent
/// - Critical errors such as [`AckedError`] coming from the processing pipeline
///
/// ## Example
///
/// ```no_run
/// use futures_util::StreamExt;
/// use p2panda_core::Topic;
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// # let node = p2panda::spawn().await?;
/// let topic = Topic::random();
///
/// let (_tx, mut rx) = node.stream::<String>(topic).await?;
///
/// while let Some(stream_event) = rx.next().await {
///     // .. react to topic stream events
/// }
/// #
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct StreamSubscription<M> {
    topic: Topic,
    store: SqliteStore,
    acked: Acked,
    #[allow(unused)]
    sync_handle: Arc<SyncHandle<Operation, TopicLogSyncEvent<Extensions>>>,
    stream: ReceiverStream<StreamEvent<M>>,
}

impl<M> StreamSubscription<M> {
    /// Associated topic.
    pub fn topic(&self) -> Topic {
        self.topic
    }

    /// Explicitly acknowledge operation.
    ///
    /// Fails silently if operation is not known (it might have been pruned, etc.).
    ///
    /// If the [`AckPolicy`] is set to "explicit", users want to call this method _after_
    /// applicaton-level processing has successfully finished. See high-level description in
    /// [`Node::stream`](crate::node::Node::stream) for more details.
    pub async fn ack(&self, id: Hash) -> Result<(), AckedError> {
        if let Some(operation) = OperationStore::<_, _>::get_operation(&self.store, &id).await? {
            self.acked.ack(&operation.header).await?;
        }

        Ok(())
    }
}

impl<M> Stream for StreamSubscription<M>
where
    M: Serialize + for<'a> Deserialize<'a> + Send + 'static,
{
    type Item = StreamEvent<M>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.stream.poll_next_unpin(cx)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.stream.size_hint()
    }
}

/// Operations with application messages, system events and errors coming from a topic stream
/// subscription.
#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum StreamEvent<M> {
    /// Operation with application message coming from a topic stream.
    ///
    /// Operations can arrive from various sources. For example, from a sync session with a remote
    /// node, a locally created message or import.
    Processed {
        /// Processed operation.
        operation: ProcessedOperation<M>,

        /// The source of the operation.
        source: Source,
    },

    /// Sync session started with a remote node.
    SyncStarted {
        /// Id of the remote sending node.
        remote_node_id: NodeId,

        /// Id of the sync session.
        session_id: u64,

        /// Total operations which will be received during this session.
        incoming_operations: u32,

        /// Total operations which will be sent during this session.
        outgoing_operations: u32,

        /// Total bytes which will be received during this session.
        incoming_bytes: u32,

        /// Total bytes which will be sent during this session.
        outgoing_bytes: u32,

        /// Total sessions currently running over the same topic.
        topic_sessions: u32,
    },

    /// Sync session ended with a remote node.
    SyncEnded {
        /// Id of the remote sending node.
        remote_node_id: NodeId,

        /// Id of the sync session.
        session_id: u64,

        /// Operation sent during this session.
        sent_operations: u32,

        /// Operations received during this session.
        received_operations: u32,

        /// Bytes sent during this session.
        sent_bytes: u32,

        /// Bytes received during this session.
        received_bytes: u32,

        /// Total bytes sent for this topic across all sessions.
        sent_bytes_topic_total: u32,

        /// Total bytes received for this topic across all sessions.
        received_bytes_topic_total: u32,

        /// If the sync session ended with an error the reason is included here.
        error: Option<SyncError>,
    },

    /// Import of operations from an external stream has started.
    ImportStarted {
        /// Id of the import session.
        session_id: SessionId,
    },

    /// Import of operations from an external stream has ended.
    ImportEnded {
        /// Id of the import session.
        session_id: SessionId,
    },

    /// Operation failed during event processing of the system-level pipeline.
    ///
    /// This is likely to come from either processing invalid operations from a broken / malicious
    /// node or due to a bug in the Node API.
    ProcessingFailed {
        /// Event which failed during system-level processing.
        ///
        /// Inspect the event to find the cause of the failure.
        event: Event,

        /// Error which occurred during processing.
        error: ProcessorError,

        /// The source of the operation.
        source: Source,
    },

    /// Re-playing of events in topic stream started.
    ReplayStarted {
        /// Number of operations to-be replayed.
        total_operations: u32,
    },

    /// Re-playing of events in topic stream ended.
    ReplayEnded,

    /// Topic stream could not re-play events due to an internal error.
    ReplayFailed { error: Arc<ReplayError> },

    /// Deserializing the application message into the specified type failed.
    ///
    /// This is an application-level error and indicates an invalid application payload.
    //
    // TODO: Since this is an applicaton-level concern we should remove encoding / decoding from our
    // APIs. See related issue: https://github.com/p2panda/p2panda/issues/1072
    DecodeFailed { event: Event, error: DecodeError },

    /// Topic stream could not acknowledge events due to an internal error.
    AckFailed {
        event: Event,
        error: Arc<AckedError>,
    },

    /// Space has been created or modified.
    Space(SpaceEvent),

    /// Group has been created or modified.
    Group(GroupEvent<AuthCapabilities>),

    /// Key bundle has been processed.
    KeyBundle(VerifyingKey),
}

/// Processed operation with application message coming from a topic stream.
#[derive(Clone, Debug, PartialEq)]
pub struct ProcessedOperation<M> {
    event: Event,
    topic: Topic,
    acked: Acked,
    message: M,
}

impl<M> ProcessedOperation<M> {
    /// Associated topic.
    pub fn topic(&self) -> Topic {
        self.topic
    }

    /// Unique identifier of this operation.
    pub fn id(&self) -> Hash {
        self.event.hash()
    }

    /// Verified author.
    pub fn author(&self) -> VerifyingKey {
        self.event.header().verifying_key
    }

    /// Timestamp when this operation was created.
    ///
    /// Microseconds since the UNIX epoch based on system time.
    pub fn timestamp(&self) -> u64 {
        self.event.header().extensions.timestamp().into()
    }

    /// Application message.
    pub fn message(&self) -> &M {
        &self.message
    }

    /// Meta-data for inspecting and debugging the processed event (failure / success status) and
    /// underlying [`Operation`] of the append-only log.
    pub fn processed(&self) -> &Event {
        &self.event
    }

    /// Acknowledge this event.
    ///
    /// If the [`AckPolicy`] is set to "explicit", users want to call this method _after_
    /// applicaton-level processing has successfully finished. See high-level description in
    /// [`Node::stream`](crate::node::Node::stream) for more details.
    ///
    /// ## Example
    ///
    /// ```no_run
    /// # use futures_util::StreamExt;
    /// # use p2panda::node::AckPolicy;
    /// # use p2panda::streams::StreamEvent;
    /// # use p2panda_core::Topic;
    /// # use serde::{Serialize, Deserialize};
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let node = p2panda::builder()
    ///     .ack_policy(AckPolicy::Explicit)
    ///     .spawn()
    ///     .await?;
    ///
    /// let topic = Topic::random();
    ///
    /// let (_tx, mut rx) = node.stream::<Vec<u8>>(topic).await?;
    ///
    /// while let Some(StreamEvent::Processed { operation, .. }) = rx.next().await {
    ///     // Do things with the message, event sourcing, apply to CRDT, materialise state,
    ///     // write to database, ..
    ///     let crdt = operation.message();
    ///
    ///     // Finally, acknowledge it.
    ///     operation.ack().await?;
    /// }
    /// #
    /// # Ok(())
    /// # }
    /// ```
    pub async fn ack(&self) -> Result<(), AckedError> {
        self.acked.ack(self).await?;
        Ok(())
    }
}

impl<M> Borrow<Header> for &ProcessedOperation<M> {
    fn borrow(&self) -> &Header {
        self.event.header()
    }
}

/// Source of a processed operation.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum Source {
    /// Source when an operation arrived via a sync session with a remote node.
    SyncSession {
        /// Id of the remote sending node.
        remote_node_id: NodeId,

        /// Id of the sync session.
        session_id: u64,

        /// Operation sent during this session.
        sent_operations: u32,

        /// Operations received during this session.
        received_operations: u32,

        /// Bytes sent during this session.
        sent_bytes: u32,

        /// Bytes received during this session.
        received_bytes: u32,

        /// Total bytes sent for this topic across all sessions.
        sent_bytes_topic_total: u32,

        /// Total bytes received for this topic across all sessions.
        received_bytes_topic_total: u32,

        /// The session phase during which an operation arrived.
        phase: SessionPhase,
    },

    /// Source when an operation arrived by an external stream (eg. reading from the filesystem or
    /// a remote service).
    ExternalStream {
        /// Id of the import session.
        session_id: u64,
    },

    /// Source when an operation was published locally or replayed.
    LocalStore,
}

/// Error occurred when creating operation and publishing it to topic stream.
#[derive(Debug, Error)]
pub enum PublishError {
    #[error("an error occurred while serializing the message for publication: {0}")]
    MessageEncoding(#[from] EncodeError),

    #[error("an error occurred while creating an operation in the forge: {0}")]
    Forge(#[from] ForgeError),

    #[error("an error occurred while publishing an operation to the log sync stream: {0}")]
    SyncHandle(String),

    #[error("could not send to processor pipeline: {0}")]
    SendToProcessor(String),
}

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("could not send to processor pipeline: {0}")]
    SendToProcessor(String),

    #[error("an error occurred awaiting message from processor: {0}")]
    ReceiveFromProcessor(String),
}
