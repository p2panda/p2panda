// SPDX-License-Identifier: MIT OR Apache-2.0

use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::{Stream, StreamExt, ready};
use p2panda_core::cbor::decode_cbor;
use p2panda_core::{Hash, PublicKey, Topic};
use p2panda_net::NodeId;
use p2panda_net::sync::{SyncHandle, SyncHandleError};
use p2panda_stream::ingest::IngestArguments;
use p2panda_sync::protocols::TopicLogSyncEvent;
use pin_project::pin_project;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::broadcast;
use tokio::task::{JoinHandle, LocalSet};
use tokio_stream::wrappers::BroadcastStream;

use crate::processor::Processor;
use crate::{Extensions, Header, Operation};

/// Handle onto an eventually-consistent stream, exposes API for publishing messages, subscribing
/// to the event stream, and acknowledging received messages.
pub struct StreamHandle<M> {
    topic: Topic,
    inner: SyncHandle<Operation, TopicLogSyncEvent<Extensions>>,
    sync_task: JoinHandle<()>,
    app_rx: broadcast::Receiver<StreamEvent<M>>,
}

impl<M> Drop for StreamHandle<M> {
    fn drop(&mut self) {
        self.sync_task.abort();
    }
}

impl<M> StreamHandle<M>
where
    M: Clone + Serialize + for<'a> Deserialize<'a> + Send + 'static,
{
    pub(crate) async fn new(
        topic: Topic,
        sync_handle: SyncHandle<Operation, TopicLogSyncEvent<Extensions>>,
        processor: Processor<Topic, Extensions, Topic>,
    ) -> Result<Self, SyncHandleError<Operation, TopicLogSyncEvent<Extensions>>> {
        let mut sync_stream = sync_handle.subscribe().await?;

        let (app_tx, app_rx) = broadcast::channel::<StreamEvent<M>>(128);

        let sync_task = tokio::spawn(async move {
            while let Some(result) = sync_stream.next().await {
                // Ignore internal broadcast channel error, this only indicates that the channel
                // dropped a message which we can't do much about on this layer anymore. In the
                // future we want to remove this error type altogether.
                //
                // Related issue: https://github.com/p2panda/p2panda/issues/959
                let Ok(from_sync) = result else {
                    continue;
                };

                let event = match from_sync.event {
                    TopicLogSyncEvent::Operation(operation) => {
                        // TODO: Attempt decoding application-level payload _before_ system-level
                        // processing or _after_?
                        let message = match &operation.body {
                            Some(body) => match decode_cbor::<M, _>(body.as_bytes()) {
                                Ok(message) => Some(message),
                                Err(_err) => {
                                    // TODO: I believe we want to surface a decode error to the
                                    // application-layer as it is part of the application logic.
                                    continue;
                                }
                            },
                            None => None,
                        };

                        // Send operation to processor task and wait for result. This blocks the
                        // sync stream and makes sure that all events are still in the same order.
                        //
                        // FIXME: This hangs currently forever if processing this operation failed.
                        // See comments in processor code.
                        let processed = processor
                            .process(IngestArguments {
                                operation: *operation,
                                log_id: topic,
                                topic,
                                prune_flag: false,
                            })
                            .await;

                        // Do not forward operations to the application-layer if there's no body.
                        let Some(message) = message else {
                            continue;
                        };

                        StreamEvent::Message(Message {
                            header: processed.header,
                            topic,
                            body: message,
                        })
                    }
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

                if app_tx.send(event).is_err() {
                    break;
                }
            }
        });

        Ok(Self {
            topic,
            inner: sync_handle,
            sync_task,
            app_rx,
        })
    }

    pub fn topic(&self) -> Topic {
        self.topic
    }

    /// Publish a message.
    pub async fn publish(&self, _message: M) -> Result<Hash, StreamError> {
        unimplemented!()
    }

    /// Subscribe to the message stream.
    pub async fn subscribe(
        &self,
    ) -> Result<StreamSubscription<M>, SyncHandleError<Operation, TopicLogSyncEvent<Extensions>>>
    {
        // TODO: Race-condition due to resubscribe? We likely want another API here.
        //
        // See related issue: https://github.com/p2panda/p2panda/issues/1041
        let stream = BroadcastStream::new(self.app_rx.resubscribe());

        Ok(StreamSubscription {
            topic: self.topic,
            stream,
        })
    }

    /// Explicitly acknowledge message.
    // TODO: Implementing this is not a priority right now.
    pub async fn ack(&self, _message_id: Hash) -> Result<(), StreamError> {
        // This is a no-op if messages are automatically acked (which is the default).
        unimplemented!()
    }

    /// Repeat streaming all known messages again.
    ///
    /// This can be useful if the application doesn't keep any materialised state around and needs
    /// to repeat all messages on start.
    ///
    /// Another use-case is the roll-out of an application update where all state needs to be
    /// re-materialised.
    // TODO: This will be required by applications like Reflection.
    //
    // Method will likely move somewhere else. See: https://github.com/p2panda/p2panda/issues/1042
    pub async fn replay(&self) -> Result<(), StreamError> {
        unimplemented!()
    }
}

#[derive(Debug, Clone, PartialEq)]
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
}

#[derive(Debug, Clone, PartialEq)]
pub struct Message<M> {
    header: Header,
    topic: Topic,
    body: M,
}

impl<M> Message<M> {
    pub fn topic(&self) -> Topic {
        self.topic
    }

    pub fn id(&self) -> Hash {
        self.header.hash()
    }

    pub fn author(&self) -> PublicKey {
        self.header.public_key
    }

    pub fn timestamp(&self) -> u64 {
        self.header.timestamp
    }

    pub fn header(&self) -> &Header {
        &self.header
    }

    pub fn body(&self) -> &M {
        &self.body
    }

    pub fn ack(&self) {
        unimplemented!()
    }
}

/// Subscription to events arriving from a stream.
#[pin_project]
pub struct StreamSubscription<M> {
    topic: Topic,
    #[pin]
    stream: BroadcastStream<StreamEvent<M>>,
}

impl<M> StreamSubscription<M>
where
    M: Serialize + for<'a> Deserialize<'a>,
{
    pub fn topic(&self) -> Topic {
        self.topic
    }
}

impl<M> Stream for StreamSubscription<M>
where
    M: Clone + Serialize + for<'a> Deserialize<'a> + Send + 'static,
{
    type Item = StreamEvent<M>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match ready!(self.stream.poll_next_unpin(cx)) {
            Some(Ok(item)) => Poll::Ready(Some(item)),
            Some(Err(_)) => Poll::Pending,
            None => Poll::Ready(None),
        }
    }
}

#[derive(Debug, Error)]
pub enum StreamError {}
