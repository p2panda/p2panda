// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_util::{FutureExt, Stream, StreamExt};
use p2panda_core::cbor::{DecodeError, EncodeError, decode_cbor, encode_cbor};
use p2panda_core::traits::Digest;
use p2panda_core::{Hash, PublicKey, Topic};
use p2panda_net::NodeId;
use p2panda_net::sync::{SyncHandle, SyncHandleError};
use p2panda_store::SqliteStore;
use p2panda_sync::protocols::TopicLogSyncEvent;
use pin_project::pin_project;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;
use tracing::warn;

use crate::forge::{Forge, ForgeError, OperationForge};
use crate::node::AckPolicy;
use crate::operation::{Extensions, LogId, Operation};
use crate::processor::{Event, Pipeline};
use crate::streams::Offset;
use crate::streams::replay::replay_from_start;

/// Number of items which can stay in the buffer before the application-layer picks up the
/// operations. If buffer runs full the processor will pause work and we'll apply backpressure to
/// the sync backend.
const BUFFER_SIZE: usize = 16;

/// Number of items which can stay in the buffer before processing kicks in for locally published
/// items. If buffer runs full, creating new operations will apply backpressure.
const PUBLISH_BUFFER_SIZE: usize = 128;

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
pub(crate) async fn processed_stream<M>(
    topic: Topic,
    ack_policy: AckPolicy,
    sync_handle: SyncHandle<Operation, TopicLogSyncEvent<Extensions>>,
    store: SqliteStore,
    forge: OperationForge,
    pipeline: Pipeline<LogId, Extensions, Topic>,
    offset: Offset,
) -> Result<
    (StreamPublisher<M>, StreamSubscription<M>),
    SyncHandleError<Operation, TopicLogSyncEvent<Extensions>>,
>
where
    M: Serialize + for<'a> Deserialize<'a> + Send + 'static,
{
    let mut sync_stream = sync_handle.subscribe().await?;

    // Channel to send processed events to the application-layer.
    let (app_tx, app_rx) = mpsc::channel::<StreamEvent<M>>(BUFFER_SIZE);

    // Channel to send locally created operations to the processing pipeline. A "oneshot" callback
    // is attached to allow publishers to await the processing result.
    let (publish_tx, mut publish_rx) = mpsc::channel::<(
        Operation,
        Option<M>,
        oneshot::Sender<Event<LogId, Extensions, Topic>>,
    )>(PUBLISH_BUFFER_SIZE);

    {
        let pipeline = pipeline.clone();

        tokio::spawn(async move {
            // In the case where a full replay of the topic stream is requested, then spawn a
            // replay task and await it's completion. This will block processing of the sync stream
            // and of locally created operations until it is complete.
            if offset == Offset::Start {
                let result = replay_from_start(
                    topic,
                    store.clone(),
                    app_tx.clone(),
                    pipeline.clone(),
                    ack_policy,
                )
                .await;

                // Errors occurring in the replay task which be returned to the user.
                if let Err(err) = result {
                    warn!("error occurred in replay task: {err:?}");

                    let _ = app_tx
                        .send(StreamEvent::ReplayFailed {
                            topic,
                            error: err.to_string(),
                        })
                        .await;
                }
            }

            loop {
                let event = tokio::select! {
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

                        match from_sync.event {
                            TopicLogSyncEvent::Operation(operation) => {
                                let Some(event) = process_operation::<M>(
                                    *operation.clone(),
                                    topic,
                                    &pipeline,
                                    ack_policy
                                ).await else {
                                    continue;
                                };

                                event
                            }
                            // TODO: Correctly handle log sync events.
                            TopicLogSyncEvent::SyncStatus(metrics) => StreamEvent::SyncStarted {
                                remote_node_id: from_sync.remote,
                                session_id: from_sync.session_id,
                                incoming_operations: metrics.total_operations_remote
                                    .unwrap_or_default(),
                                outgoing_operations: metrics.total_operations_local
                                    .unwrap_or_default(),
                                incoming_bytes: metrics.total_bytes_remote
                                    .unwrap_or_default(),
                                outgoing_bytes: metrics.total_bytes_local
                                    .unwrap_or_default(),
                            },
                            TopicLogSyncEvent::Success => StreamEvent::SyncEnded {
                                remote_node_id: from_sync.remote,
                                session_id: from_sync.session_id,
                            },
                            TopicLogSyncEvent::Failed { .. } => StreamEvent::SyncEnded {
                                remote_node_id: from_sync.remote,
                                session_id: from_sync.session_id,
                            },
                            _ => continue,
                        }
                    }

                    // Received locally created operation which needs to be processed as well.
                    //
                    // If the publishing channel gets closed, for example when the publisher handle
                    // got dropped, we still continue with this task, as we still might receive
                    // operations from the log sync stream.
                    Some((operation, message, processed_tx)) = publish_rx.recv() => {
                        let event = process_published_operation(
                            operation,
                            topic,
                            &pipeline,
                        ).await;

                        // Inform publisher optionally about result of processor and that we're
                        // done here.
                        let _ = processed_tx.send(event.clone());

                        if let Some(message) = message {
                            StreamEvent::Processed(ProcessedOperation {
                                event,
                                topic,
                                message,
                            })
                        } else {
                            continue;
                        }
                    }
                };

                // Send processing result to application layer.
                //
                // If channel stopped working because the subscriber got dropped, ignore it as we
                // still might want to process locally published operations.
                let _ = app_tx.send(event).await;
            }
        });
    }

    // Keep around the sync handle on both the publisher and subscriber ends to keep it running
    // even if one half got dropped.
    let sync_handle = Arc::new(sync_handle);

    let tx = StreamPublisher {
        topic,
        sync_handle: sync_handle.clone(),
        forge,
        publish_tx,
        _marker: PhantomData,
    };

    let rx = StreamSubscription {
        topic,
        sync_handle,
        stream: ReceiverStream::new(app_rx),
    };

    Ok((tx, rx))
}

/// Process an incoming operation coming from an external stream (sync- or replay task).
pub(crate) async fn process_operation<M>(
    operation: Operation,
    topic: Topic,
    pipeline: &Pipeline<LogId, Extensions, Topic>,
    ack_policy: AckPolicy,
) -> Option<StreamEvent<M>>
where
    M: Serialize + for<'a> Deserialize<'a> + Send + 'static,
{
    let log_id = LogId::from_topic(topic);
    let prune_flag = operation.header.extensions.prune_flag;

    // Send operation to processor task and wait for result. This blocks any parent stream and
    // makes sure that all events are handled in same order.
    let event = pipeline
        .process(Event::new(operation, log_id, topic, prune_flag))
        .await;

    // Do not forward operations which failed processing on system-level. We do _not_ forward the
    // error to application-level, only log an error.
    if event.is_failed() {
        warn!(
            id = %event.hash(),
            "processing operation failed: {}",
            event.failure_reason().expect("error")
        );

        return None;
    }

    // Do not forward operations to the application-layer if there's no body.
    let body = event.body()?;

    // Attempt decoding application-layer message. This takes place _after_ system-level processing
    // completed and the operation was ingested.
    //
    // In case decoding fails due to an application bug, users have the option to re-play this
    // persisted operation and attempt decoding again.
    //
    // If application data is malformed users can choose to remove the payload of the operation or
    // delete the whole log altogether.
    //
    // TODO: Is this mixing up concerns? We can only handle bytes on our end and let the users do
    // decoding on application layer?
    let event = match decode_cbor::<M, _>(body.as_bytes()) {
        Ok(message) => StreamEvent::Processed(ProcessedOperation {
            event,
            topic,
            message,
        }),
        Err(err) => StreamEvent::DecodingFailed {
            event,
            topic,
            error: err,
        },
    };

    if ack_policy == AckPolicy::Automatic {
        // TODO: Automatically acknowledge this message.
    }

    Some(event)
}

/// Process an operation which was just published locally.
///
/// This is different from processing a remote or re-played operation coming from a stream:
///
/// 1. Since we know the message already we don't need to decode it.
/// 2. We want to inform the publisher about the result of the processing if they want to. This is
///    different from processing operations coming from an external stream where there's no
///    explicit user call which created them.
pub(crate) async fn process_published_operation(
    operation: Operation,
    topic: Topic,
    pipeline: &Pipeline<LogId, Extensions, Topic>,
) -> Event<LogId, Extensions, Topic> {
    let log_id = LogId::from_topic(topic);
    let prune_flag = operation.header.extensions.prune_flag;

    // Send operation to processor task and wait for result. This blocks any parent stream and
    // makes sure that all events are handled in same order.
    let event = pipeline
        .process(Event::new(operation, log_id, topic, prune_flag))
        .await;

    if event.is_failed() {
        warn!(
            id = %event.hash(),
            "processing local operation failed: {}",
            event.failure_reason().expect("error")
        );
    }

    // TODO: Always ack your own operations?

    event
}

#[derive(Clone)]
pub struct StreamPublisher<M> {
    topic: Topic,
    sync_handle: Arc<SyncHandle<Operation, TopicLogSyncEvent<Extensions>>>,
    forge: OperationForge,
    #[allow(clippy::type_complexity)]
    publish_tx: mpsc::Sender<(
        Operation,
        Option<M>,
        oneshot::Sender<Event<LogId, Extensions, Topic>>,
    )>,
    _marker: PhantomData<M>,
}

impl<M> StreamPublisher<M>
where
    M: Serialize,
{
    pub fn topic(&self) -> Topic {
        self.topic
    }

    /// Publish a message.
    pub async fn publish(&self, message: M) -> Result<PublishFuture, PublishError> {
        self.publish_inner(Some(message), false).await
    }

    /// Deletes all our previously published messages in this stream.
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

    async fn publish_inner(
        &self,
        message: Option<M>,
        prune_flag: bool,
    ) -> Result<PublishFuture, PublishError> {
        // Create, sign and persist operation with given payload.
        let extensions = Extensions::from_topic(self.topic()).prune_flag(prune_flag);

        let body_bytes = match message {
            Some(ref message) => Some(encode_cbor(&message)?),
            None => None,
        };

        let operation = self
            .forge
            .create_operation(self.topic(), extensions.log_id, body_bytes, extensions)
            .await?
            .ok_or(PublishError::DuplicateOperation)?;
        let hash = operation.hash;

        // Start processing operation in pipeline. Keep an oneshot receiver around to allow users
        // to optionally await & get informed when processing has finished.
        let (processed_tx, processed_rx) = oneshot::channel();
        self.publish_tx
            .send((operation.clone(), message, processed_tx))
            .await
            .map_err(|err| PublishError::SendToProcessor(err.to_string()))?;

        // Try pushing operation to other nodes if we have an active and "live" sync session with
        // them. This allows disseminating new messages fastly in the network.
        //
        // If no active live session exists, nodes will pick up the operation later when running
        // the sync protocol.
        self.sync_handle
            .publish(operation)
            .await
            .map_err(|err| PublishError::SyncHandle(err.to_string()))?;

        Ok(PublishFuture { hash, processed_rx })
    }
}

/// Future which can be awaited to find out when locally published operation has finished
/// processing.
#[derive(Debug)]
pub struct PublishFuture {
    hash: Hash,
    processed_rx: oneshot::Receiver<Event<LogId, Extensions, Topic>>,
}

impl PublishFuture {
    /// Returns hash of the published operation.
    pub fn hash(&self) -> Hash {
        self.hash
    }
}

impl Future for PublishFuture {
    type Output = Result<Event<LogId, Extensions, Topic>, oneshot::error::RecvError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.processed_rx.poll_unpin(cx)
    }
}

/// Subscription to events arriving from a stream.
#[pin_project]
pub struct StreamSubscription<M> {
    topic: Topic,
    #[allow(unused)]
    sync_handle: Arc<SyncHandle<Operation, TopicLogSyncEvent<Extensions>>>,
    #[pin]
    stream: ReceiverStream<StreamEvent<M>>,
}

impl<M> StreamSubscription<M> {
    /// Explicitly acknowledge message.
    pub async fn ack(&self, _message_id: Hash) {
        // This is a no-op if messages are automatically acked (which is the default).
        unimplemented!()
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
}

#[derive(Clone, Debug)]
pub enum StreamEvent<M> {
    Processed(ProcessedOperation<M>),
    SyncStarted {
        remote_node_id: NodeId,
        session_id: u64,
        incoming_operations: u64,
        outgoing_operations: u64,
        incoming_bytes: u64,
        outgoing_bytes: u64,
    },
    SyncEnded {
        remote_node_id: NodeId,
        session_id: u64,
    },
    DecodingFailed {
        event: Event<LogId, Extensions, Topic>,
        topic: Topic,
        error: DecodeError,
    },
    ReplayFailed {
        topic: Topic,
        error: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProcessedOperation<M> {
    event: Event<LogId, Extensions, Topic>,
    topic: Topic,
    message: M,
}

impl<M> ProcessedOperation<M> {
    pub fn topic(&self) -> Topic {
        self.topic
    }

    pub fn id(&self) -> Hash {
        self.event.hash()
    }

    pub fn author(&self) -> PublicKey {
        self.event.header().public_key
    }

    pub fn timestamp(&self) -> u64 {
        self.event.header().timestamp.into()
    }

    pub fn message(&self) -> &M {
        &self.message
    }

    pub fn processed(&self) -> &Event<LogId, Extensions, Topic> {
        &self.event
    }

    pub async fn ack(&self) {
        unimplemented!()
    }
}

#[derive(Debug, Error)]
pub enum PublishError {
    #[error("an error occurred while serializing the message for publication: {0}")]
    MessageEncoding(#[from] EncodeError),

    #[error("an error occurred while creating an operation in the forge: {0}")]
    Forge(#[from] ForgeError),

    #[error("message already exists in the forge")]
    DuplicateOperation,

    #[error("an error occurred while publishing an operation to the log sync stream: {0}")]
    SyncHandle(String),

    #[error("could not send operation to processor pipeline: {0}")]
    SendToProcessor(String),
}
