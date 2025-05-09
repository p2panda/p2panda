// SPDX-License-Identifier: MIT OR Apache-2.0

use tokio::sync::mpsc;
use tokio::sync::mpsc::error::{SendError, TrySendError};
use tracing::warn;

use crate::engine::engine::ToEngineActor;
use crate::engine::topic_streams::TopicChannelType;
use crate::network::ToNetwork;

/// Send bytes associated with a specific topic into the network.
///
/// `TopicSender` acts as a thin wrapper around
/// [`tokio::sync::mpsc::Sender`](https://docs.rs/tokio/latest/tokio/sync/mpsc/struct.Sender.html),
/// only implementing a limited subset of methods.
///
/// Unsubscribe behaviour for the topic is automatically invoked when the sender is dropped. The
/// state of all senders and receivers for each subscribed topic is tracked internally. A topic is
/// only fully unsubscribed from when _all_ senders and receivers for that topic have been dropped.
/// In practice, this means that you can drop a sender or receiver and continue interacting with
/// the other half of the channel. Or you can subscribe to the same topic twice, drop one
/// sender-receiver pair and continue to use the other pair.
#[derive(Debug)]
pub struct TopicSender<T> {
    topic: Option<T>,
    stream_id: usize,
    to_network_tx: mpsc::Sender<ToNetwork>,
    engine_actor_tx: mpsc::Sender<ToEngineActor<T>>,
}

impl<T> TopicSender<T> {
    pub(crate) fn new(
        topic: T,
        stream_id: usize,
        to_network_tx: mpsc::Sender<ToNetwork>,
        engine_actor_tx: mpsc::Sender<ToEngineActor<T>>,
    ) -> Self {
        Self {
            topic: Some(topic),
            stream_id,
            to_network_tx,
            engine_actor_tx,
        }
    }

    pub async fn send(&self, to_network_bytes: ToNetwork) -> Result<(), SendError<ToNetwork>> {
        self.to_network_tx.send(to_network_bytes).await?;

        Ok(())
    }

    pub fn try_send(&self, to_network_bytes: ToNetwork) -> Result<(), TrySendError<ToNetwork>> {
        self.to_network_tx.try_send(to_network_bytes)?;

        Ok(())
    }

    pub async fn closed(&self) {
        self.to_network_tx.closed().await
    }

    pub fn is_closed(&self) -> bool {
        self.to_network_tx.is_closed()
    }
}

impl<T> Drop for TopicSender<T> {
    fn drop(&mut self) {
        if let Some(topic) = self.topic.take() {
            if self
                .engine_actor_tx
                .try_send(ToEngineActor::UnsubscribeTopic {
                    topic,
                    stream_id: self.stream_id,
                    channel_type: TopicChannelType::Sender,
                })
                .is_err()
            {
                warn!("engine actor receiver dropped before topic unsubscribe event could be sent")
            }
        }
    }
}
