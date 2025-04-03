// SPDX-License-Identifier: MIT OR Apache-2.0

use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::Stream;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TryRecvError;
use tracing::warn;

use crate::engine::engine::ToEngineActor;
use crate::engine::topic_streams::TopicChannelType;
use crate::network::FromNetwork;

/// Receive bytes associated with a specific topic from the network.
///
/// `TopicReceiver` acts as a thin wrapper around [`tokio::sync::mpsc::Receiver`], only
/// implementing a limited subset of methods, and invokes unsubscribe behaviour for the topic when
/// dropped. The state of all senders and receivers for the topic is tracked internally; the topic
/// is only fully unsubscribed from when all of them have been dropped.
#[derive(Debug)]
pub struct TopicReceiver<T> {
    topic: Option<T>,
    stream_id: usize,
    from_network_rx: mpsc::Receiver<FromNetwork>,
    engine_actor_tx: mpsc::Sender<ToEngineActor<T>>,
}

impl<T> TopicReceiver<T> {
    pub(crate) async fn new(
        topic: T,
        stream_id: usize,
        from_network_rx: mpsc::Receiver<FromNetwork>,
        engine_actor_tx: mpsc::Sender<ToEngineActor<T>>,
    ) -> Self {
        Self {
            topic: Some(topic),
            stream_id,
            from_network_rx,
            engine_actor_tx,
        }
    }

    pub async fn recv(&mut self) -> Option<FromNetwork> {
        self.from_network_rx.recv().await
    }

    pub async fn recv_many(&mut self, buffer: &mut Vec<FromNetwork>, limit: usize) -> usize {
        self.from_network_rx.recv_many(buffer, limit).await
    }

    pub fn try_recv(&mut self) -> Result<FromNetwork, TryRecvError> {
        self.from_network_rx.try_recv()
    }

    pub fn close(&mut self) {
        self.from_network_rx.close()
    }

    pub fn is_closed(&self) -> bool {
        self.from_network_rx.is_closed()
    }

    pub fn poll_recv(&mut self, cx: &mut Context<'_>) -> Poll<Option<FromNetwork>> {
        self.from_network_rx.poll_recv(cx)
    }
}

impl<T> Drop for TopicReceiver<T> {
    fn drop(&mut self) {
        if let Some(topic) = self.topic.take() {
            if self
                .engine_actor_tx
                .try_send(ToEngineActor::UnsubscribeTopic {
                    topic,
                    stream_id: self.stream_id,
                    channel_type: TopicChannelType::Receiver,
                })
                .is_err()
            {
                warn!("engine actor receiver dropped before topic unsubscribe event could be sent")
            }
        }
    }
}

impl<T> Unpin for TopicReceiver<T> {}

/// A wrapper around [`TopicReceiver`] that implements [`Stream`].
#[derive(Debug)]
pub struct TopicReceiverStream<T> {
    inner: TopicReceiver<T>,
}

impl<T> TopicReceiverStream<T> {
    /// Create a new `TopicReceiverStream`.
    pub fn new(recv: TopicReceiver<T>) -> Self {
        Self { inner: recv }
    }

    /// Get back the inner `TopicReceiver`.
    pub fn into_inner(self) -> TopicReceiver<T> {
        self.inner
    }

    /// Closes the receiving half of a channel without dropping it.
    ///
    /// This prevents any further messages from being sent on the channel while
    /// still enabling the receiver to drain messages that are buffered. Any
    /// outstanding [`Permit`] values will still be able to send messages.
    ///
    /// To guarantee no messages are dropped, after calling `close()`, you must
    /// receive all items from the stream until `None` is returned.
    pub fn close(&mut self) {
        self.inner.close();
    }
}

impl<T> Stream for TopicReceiverStream<T> {
    type Item = FromNetwork;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.poll_recv(cx)
    }
}

impl<T> AsRef<TopicReceiver<T>> for TopicReceiverStream<T> {
    fn as_ref(&self) -> &TopicReceiver<T> {
        &self.inner
    }
}

impl<T> AsMut<TopicReceiver<T>> for TopicReceiverStream<T> {
    fn as_mut(&mut self) -> &mut TopicReceiver<T> {
        &mut self.inner
    }
}

impl<T> From<TopicReceiver<T>> for TopicReceiverStream<T> {
    fn from(recv: TopicReceiver<T>) -> Self {
        Self::new(recv)
    }
}
