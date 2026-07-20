// SPDX-License-Identifier: MIT OR Apache-2.0

use std::borrow::Borrow;
use std::fmt::Debug;
use std::sync::Arc;

use futures_util::StreamExt;
use futures_util::stream::BoxStream;
use p2panda_core::cbor::{DecodeError, decode_cbor};
use p2panda_core::traits::Digest;
use p2panda_core::{Hash, Topic, VerifyingKey};
use p2panda_net::NodeId;
use p2panda_net::sync::SyncHandle;
// TODO: Replace with ShortFormat from p2panda-core.
// See: https://github.com/p2panda/p2panda/issues/1270
use p2panda_net::utils::ShortFormat;
use p2panda_spaces::SpaceEvent;
use p2panda_store::SqliteStore;
use p2panda_stream::spaces::SpacesResult;
use p2panda_sync::protocols::TopicLogSyncEvent;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use crate::forge::OperationForge;
use crate::node::{AckPolicy, CreateStreamError};
use crate::operation::{Extensions, Header, Operation};
use crate::processor::{ProcessorError, ProcessorStatus};
use crate::spaces::spawn_repair_task;
use crate::spaces::types::SpacesManager;
use crate::streams::acked::{Acked, AckedError};
use crate::streams::drop_guard::StreamDropGuard;
use crate::streams::external_stream::{
    ExternalStream, ExternalStreamEvent, ExternalStreamFuture, SessionId,
};
use crate::streams::local_stream::{LocalStream, LocalStreamEvent, LocalStreamFuture};
use crate::streams::publisher::StreamPublisher;
use crate::streams::replay::{ReplayError, StreamFrom, replay_log_ranges};
use crate::streams::subscription::StreamSubscription;
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
    let (import_external_tx, mut import_external_rx) = mpsc::channel::<(
        BoxStream<'static, Operation>,
        oneshot::Sender<ExternalStreamFuture>,
    )>(IMPORT_BUFFER_SIZE);

    // Set of currently active external streams.
    let mut external_stream = ExternalStream::default();

    // Channel for importing local operation streams.
    let (import_local_tx, mut import_local_rx) = mpsc::channel::<(
        BoxStream<'static, Operation>,
        oneshot::Sender<LocalStreamFuture>,
    )>(IMPORT_BUFFER_SIZE);

    // Set of currently active local streams.
    let mut local_stream = LocalStream::default();

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
        import_local_tx.clone(),
        to_output_tx.clone(),
        repair_rx,
    );

    // Create a cancellation token which is used to break out of the input and output event
    // processing tasks once all instances of the `StreamPublisher` and `StreamSubscription` have
    // been dropped.
    //
    // The spawn repair task doesn't require a cancellation token; it is aborted via the handle
    // during the wind-down of the input event task.
    let cancellation_token = CancellationToken::new();

    // Spawn first task which receives processed "output events" from the processing pipeline, the
    // result is handled (acking, decoding, conversion to `StreamEvent`, etc.) and then finally
    // forwarded to the application layer.
    {
        let pipeline = pipeline.clone();
        let acked = acked.clone();

        let cancellation_token_child = cancellation_token.child_token();

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

                    // Break out of the loop, thereby ending the task, when both the
                    // `StreamPublisher` and `StreamSubscription` have been dropped.
                    _ = cancellation_token_child.cancelled() => {
                            debug!(topic = %topic.to_hex(), "aborting output event processing task");
                        break
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
        let to_output_tx = to_output_tx.clone();

        let cancellation_token_child = cancellation_token.child_token();

        tokio::spawn(async move {
            // =======================
            // 1. Re-play local events
            // =======================

            {
                // This will block processing of the sync stream and of locally created operations
                // until it is complete.
                let replay_result = replay_log_ranges(
                    topic,
                    &store,
                    &to_output_tx,
                    &pipeline,
                    &sync_handle,
                    nacked_log_ranges,
                )
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
                                process_operation_in(*operation, source, topic, &pipeline, &sync_handle).await;
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
                            &sync_handle
                        ).await;

                        // Inform publisher optionally about result of processor and that we're
                        // done here.
                        let _ = processed_tx.send(event.clone());

                        continue;
                    }

                    // Receive imported external source of operations.
                    Some((stream, ready_tx)) = import_external_rx.recv() => {
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
                                process_operation_in(
                                    *operation,
                                    Source::ExternalStream { session_id },
                                    topic,
                                    &pipeline,
                                    &sync_handle
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

                    // Receive imported local source of operations.
                    Some((stream, ready_tx)) = import_local_rx.recv() => {
                        let local_stream_future = local_stream.insert(stream);
                        if ready_tx.send(local_stream_future).is_err() {
                            warn!("failed sending on local import ready channel")
                        };
                        continue;
                    }

                    // Receive the next ready event from any imported local source.
                    Some(event) = local_stream.next() => {
                        match event {
                            LocalStreamEvent::Operation(operation) => {
                                process_operation_in(
                                    *operation,
                                    Source::LocalStore,
                                    topic,
                                    &pipeline,
                                    &sync_handle
                                ).await;

                                continue;
                            },
                            LocalStreamEvent::End =>
                                vec![]
                            ,
                        }
                    },

                    // Break out of the loop, thereby ending the task, when both the
                    // `StreamPublisher` and `StreamSubscription` have been dropped.
                    _ = cancellation_token_child.cancelled() => {
                            debug!(topic = %topic.to_hex(), "aborting input event processing task");
                        break
                    }
                };

                let _ = to_output_tx.send(stream_events).await;
            }

            // Abort the repair task.
            repair_task_handle.abort();
        });
    }

    let drop_guard = StreamDropGuard::new(topic, cancellation_token.clone());
    let tx = StreamPublisher::new(
        topic,
        forge,
        sync_handle,
        publish_tx,
        import_external_tx,
        import_local_tx,
        repair_tx,
        to_output_tx,
        drop_guard.clone(),
    );
    let rx = StreamSubscription::new(topic, store, acked, ReceiverStream::new(app_rx), drop_guard);

    Ok((tx, rx))
}

/// Process an incoming operation in the pipeline.
pub(crate) async fn process_operation_in(
    operation: Operation,
    source: Source,
    topic: Topic,
    pipeline: &Pipeline,
    sync_handle: &Arc<SyncHandle<Operation, TopicLogSyncEvent<Extensions>>>,
) -> Event {
    let log_id = operation.header.extensions.log_id();
    let prune_flag = operation.header.extensions.prune_flag();
    let spaces_args = operation.header.extensions.spaces_args();

    match source {
        Source::ExternalStream { .. } | Source::LocalStore
            // Try pushing operation to other nodes if we have an active and
            // "live" sync session with them. This allows disseminating new
            // messages quickly in the network.
            //
            // If no active live session exists, nodes will pick up the
            // operation later when running the sync protocol.
            if sync_handle.publish(operation.clone()).is_err() => {
                warn!(
                    operation_id = %operation.hash(),
                    "failed sending operation on sync handle"
                )
            }
        _ => (),
    };

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

                p2panda_spaces::Event::Group(_) => {
                    // @TODO: It's not clear if group events should be forwarded on the spaces
                    // stream, so for now we don't forward any.
                    // forward_events.push(StreamEvent::Group(group_event.to_owned()))
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

    // @TODO: It's not clear where group events should be forwarded so we don't send any for now
    // and therefore this variant is commented out.
    //
    /// Group has been created or modified.
    // Group(GroupEvent<AuthCapabilities>),

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
