// SPDX-License-Identifier: MIT OR Apache-2.0

use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::Stream;
use p2panda_core::{Hash, PublicKey, Topic};
use p2panda_net::sync::LogSyncError;
use p2panda_net::sync::SyncHandle;
use p2panda_net::sync::SyncHandleError;
use p2panda_net::sync::SyncSubscription;
use p2panda_sync::protocols::TopicLogSyncEvent;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::OnceCell;

use crate::{Extensions, Header, Operation, network::Network};

/// Handle onto an eventually-consistent stream, exposes API for publishing messages, subscribing
/// to the event stream, and acknowledging received messages.
pub struct StreamHandle<M> {
    network: Network,
    topic: Topic,
    inner: OnceCell<SyncHandle<Operation, TopicLogSyncEvent<Extensions>>>,
    _marker: PhantomData<M>,
}

impl<M> StreamHandle<M> {}

impl<M> StreamHandle<M>
where
    M: Serialize + for<'a> Deserialize<'a>,
{
    async fn inner(
        &self,
    ) -> Result<&SyncHandle<Operation, TopicLogSyncEvent<Extensions>>, LogSyncError<Extensions>>
    {
        let topic = self.topic.into();
        self.inner
            .get_or_try_init(|| self.network.log_sync.stream(topic, true))
            .await
    }

    pub(crate) fn new(network: Network, topic: Topic) -> Self {
        Self {
            network,
            topic,
            inner: OnceCell::new(),
            _marker: PhantomData,
        }
    }

    pub fn topic(&self) -> Topic {
        self.topic
    }

    /// Publish a message.
    pub async fn publish(&self, _message: M) -> Result<Hash, StreamError> {
        let _inner = self.inner().await?;
        // Should be something like this
        // inner.publish(message).await?
        unimplemented!()
    }

    /// Subscribe to the message stream.
    pub fn subscribe(&self) -> StreamSubscription<M> {
        unimplemented!()
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
    pub async fn replay(&self) -> Result<(), StreamError> {
        unimplemented!()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent<M> {
    // TODO: Add more topic-related system events here which can come from node.
    Message(Message<M>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
pub struct StreamSubscription<M> {
    handle: SyncHandle<Operation, TopicLogSyncEvent<Extensions>>,
    inner: OnceCell<SyncSubscription<TopicLogSyncEvent<Extensions>>>,

    _marker: PhantomData<M>,
}

impl<M> StreamSubscription<M>
where
    M: Serialize + for<'a> Deserialize<'a>,
{
    async fn inner(
        &self,
    ) -> Result<
        &SyncSubscription<TopicLogSyncEvent<Extensions>>,
        SyncHandleError<p2panda_core::Operation<Extensions>, TopicLogSyncEvent<Extensions>>,
    > {
        self.inner.get_or_try_init(|| self.handle.subscribe()).await
    }

    pub(crate) fn new(handle: SyncHandle<Operation, TopicLogSyncEvent<Extensions>>) -> Self {
        Self {
            handle,
            inner: OnceCell::new(),
            _marker: PhantomData,
        }
    }

    pub fn topic(&self) -> Topic {
        unimplemented!()
    }
}

impl<M> Stream for StreamSubscription<M>
where
    M: Serialize + for<'a> Deserialize<'a>,
{
    type Item = Result<StreamEvent<M>, StreamError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let inner = self.inner();
        tokio::pin!(inner);

        match inner.poll(cx) {
            Poll::Ready(Ok(inner)) => {
                //tokio::pin!(stream);
                //stream.poll_next(cx)
                unimplemented!()
            }
            Poll::Ready(Err(error)) => {
                Poll::Ready(Some(Err(error.into())))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

#[derive(Debug, Error)]
pub enum StreamError {
    #[error(transparent)]
    LogSyncError(#[from] LogSyncError<Extensions>),
    #[error(transparent)]
    SyncHandleError(#[from] SyncHandleError<p2panda_core::Operation<Extensions>, TopicLogSyncEvent<Extensions>>),
}
