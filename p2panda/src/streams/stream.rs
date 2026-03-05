// SPDX-License-Identifier: MIT OR Apache-2.0

use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_util::{Stream, StreamExt};
use p2panda_core::cbor::{DecodeError, EncodeError, decode_cbor, encode_cbor};
use p2panda_core::traits::Digest;
use p2panda_core::{Hash, PublicKey, Topic};
use p2panda_net::NodeId;
use p2panda_net::sync::{SyncHandle, SyncHandleError};
use p2panda_sync::protocols::TopicLogSyncEvent;
use pin_project::pin_project;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_stream::wrappers::ReceiverStream;
use tracing::warn;

use crate::forge::{Forge, ForgeError, OperationForge};
use crate::node::AckPolicy;
use crate::processor::{Event, Pipeline};
use crate::{Extensions, Header, Offset, Operation};

/// Number of items which can stay in the buffer before the application-layer picks up the
/// operations. If buffer runs full the processor will pause work and we'll apply backpressure to
/// the sync backend.
const BUFFER_SIZE: usize = 16;

pub async fn processed_stream<M>(
    topic: Topic,
    ack_policy: AckPolicy,
    sync_handle: SyncHandle<Operation, TopicLogSyncEvent<Extensions>>,
    forge: OperationForge,
    pipeline: Pipeline<Topic, Extensions, Topic>,
    _offset: Offset,
) -> Result<
    (StreamPublisher<M>, StreamSubscription<M>),
    SyncHandleError<Operation, TopicLogSyncEvent<Extensions>>,
>
where
    M: Serialize + for<'a> Deserialize<'a> + Send + 'static,
{
    let mut sync_stream = sync_handle.subscribe().await?;

    let (app_tx, app_rx) = mpsc::channel::<StreamEvent<M>>(BUFFER_SIZE);

    // TODO: Get offset from database and re-play events first before we move on to new events.
    // This will be required by applications like Reflection.

    let sync_task = tokio::spawn(async move {
        while let Some(result) = sync_stream.next().await {
            // Ignore internal broadcast channel error, this only indicates that the channel
            // dropped a message which we can't do much about on this layer anymore. In the future
            // we want to remove this error type altogether.
            //
            // Related issue: https://github.com/p2panda/p2panda/issues/959
            let Ok(from_sync) = result else {
                continue;
            };

            let event = match from_sync.event {
                TopicLogSyncEvent::Operation(operation) => {
                    // TODO: Extract log id from operation extensions instead.
                    let log_id = topic;

                    // Send operation to processor task and wait for result. This blocks the sync
                    // stream and makes sure that all events are handled in same order.
                    let processed_event = pipeline
                        .process(Event::new(*operation, log_id, topic))
                        .await;

                    // Do not forward operations which failed processing on system-level. We do
                    // _not_ forward the error to application-level, only log an error.
                    if processed_event.is_failed() {
                        warn!(
                            id = %processed_event.hash(),
                            "processing operation failed: {}",
                            processed_event.failure_reason().expect("error")
                        );

                        continue;
                    }

                    // Do not forward operations to the application-layer if there's no body.
                    let Some(body) = processed_event.body() else {
                        continue;
                    };

                    // Attempt decoding application-layer message. This takes place _after_
                    // system-level processing completed and the operation was ingested.
                    //
                    // In case decoding fails due to an application bug, users have the option to
                    // re-play this persisted operation and attempt decoding again.
                    //
                    // If application data is malformed users can choose to remove the payload of
                    // the operation or delete the whole log altogether.
                    //
                    // TODO: Is this mixing up concerns? We can only handle bytes on our end and
                    // let the users do decoding on application layer?
                    let event = match decode_cbor::<M, _>(body.as_bytes()) {
                        Ok(message) => StreamEvent::Message(Message {
                            processed_event,
                            topic,
                            body: message,
                        }),
                        Err(err) => StreamEvent::DecodingFailed {
                            processed_event,
                            topic,
                            error: err,
                        },
                    };

                    if ack_policy == AckPolicy::Automatic {
                        // TODO: Automatically acknowledge this message.
                    }

                    event
                }
                // TODO: Correctly handle log sync events.
                TopicLogSyncEvent::SyncStatus(metrics) => StreamEvent::SyncStarted {
                    remote_node_id: from_sync.remote,
                    session_id: from_sync.session_id,
                    incoming_operations: metrics.total_operations_remote.unwrap_or_default(),
                    outgoing_operations: metrics.total_operations_local.unwrap_or_default(),
                    incoming_bytes: metrics.total_bytes_remote.unwrap_or_default(),
                    outgoing_bytes: metrics.total_bytes_local.unwrap_or_default(),
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
            };

            if app_tx.send(event).await.is_err() {
                break;
            }
        }
    });

    let tx = StreamPublisher {
        topic,
        sync_handle: Arc::new(sync_handle),
        forge,
        _marker: PhantomData,
    };

    let rx = StreamSubscription {
        topic,
        stream: ReceiverStream::new(app_rx),
        sync_task,
    };

    Ok((tx, rx))
}

#[derive(Clone)]
pub struct StreamPublisher<M> {
    topic: Topic,
    sync_handle: Arc<SyncHandle<Operation, TopicLogSyncEvent<Extensions>>>,
    forge: OperationForge,
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
    pub async fn publish(&mut self, message: M) -> Result<Hash, PublishError> {
        let encoded_message = encode_cbor(&message)?;

        let operation = self
            .forge
            .create_operation(
                self.topic(),
                self.topic(),
                Some(encoded_message),
                Extensions::default(),
            )
            .await?
            .ok_or(PublishError::DuplicateOperation)?;
        let hash = operation.hash;

        self.sync_handle
            .publish(operation)
            .await
            .map_err(|err| PublishError::SyncHandle(err.to_string()))?;

        Ok(hash)
    }
}

/// Subscription to events arriving from a stream.
#[pin_project]
pub struct StreamSubscription<M> {
    topic: Topic,
    sync_task: JoinHandle<()>,
    #[pin]
    stream: ReceiverStream<StreamEvent<M>>,
}

impl<M> StreamSubscription<M> {
    /// Explicitly acknowledge message.
    // TODO: Implementing this is not a priority right now.
    pub async fn ack(&self, _message_id: Hash) {
        // This is a no-op if messages are automatically acked (which is the default).
        unimplemented!()
    }
}

impl<M> Stream for StreamSubscription<M>
where
    M: Clone + Serialize + for<'a> Deserialize<'a> + Send + 'static,
{
    type Item = StreamEvent<M>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.stream.poll_next_unpin(cx)
    }
}

#[derive(Clone, Debug)]
pub enum StreamEvent<M> {
    Message(Message<M>),
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
        processed_event: Event<Topic, Extensions, Topic>,
        topic: Topic,
        error: DecodeError,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Message<M> {
    processed_event: Event<Topic, Extensions, Topic>,
    topic: Topic,
    body: M,
}

impl<M> Message<M> {
    pub fn topic(&self) -> Topic {
        self.topic
    }

    pub fn id(&self) -> Hash {
        self.processed_event.hash()
    }

    pub fn author(&self) -> PublicKey {
        self.processed_event.author()
    }

    pub fn timestamp(&self) -> u64 {
        self.processed_event.header().timestamp.into()
    }

    pub fn header(&self) -> &Header {
        self.processed_event.header()
    }

    // TODO: Consider better naming here. It is confusing that I have to call body on Message to
    // receive M (the "message") from an operation.
    pub fn body(&self) -> &M {
        &self.body
    }

    pub async fn ack(&self) {
        // TODO
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
}
