// SPDX-License-Identifier: MIT OR Apache-2.0

use tokio::sync::mpsc;
use tokio::sync::mpsc::error::{SendError, TrySendError};
use tracing::warn;

use crate::engine::engine::ToEngineActor;
use crate::engine::topic_streams::TopicStreamChannel;
use crate::network::ToNetwork;

// @TODO(glyph): Docs.
#[derive(Debug)]
pub struct TopicStreamSender<T> {
    topic: Option<T>,
    stream_id: usize,
    to_network_tx: mpsc::Sender<ToNetwork>,
    engine_actor_tx: mpsc::Sender<ToEngineActor<T>>,
}

impl<T> TopicStreamSender<T> {
    pub(crate) async fn new(
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

    async fn send(&mut self, to_network_bytes: ToNetwork) -> Result<(), SendError<ToNetwork>> {
        self.to_network_tx.send(to_network_bytes).await?;

        Ok(())
    }

    fn try_send(&mut self, to_network_bytes: ToNetwork) -> Result<(), TrySendError<ToNetwork>> {
        self.to_network_tx.try_send(to_network_bytes)?;

        Ok(())
    }

    async fn closed(&self) {
        self.to_network_tx.closed().await
    }

    fn is_closed(&self) -> bool {
        self.to_network_tx.is_closed()
    }
}

impl<T> Drop for TopicStreamSender<T> {
    fn drop(&mut self) {
        if let Some(topic) = self.topic.take() {
            if let Err(_) = self
                .engine_actor_tx
                .blocking_send(ToEngineActor::UnsubscribeTopic {
                    topic,
                    stream_id: self.stream_id,
                    channel_type: TopicStreamChannel::Sender,
                })
            {
                warn!("engine actor receiver dropped before topic unsubscribe event could be sent")
            }
        }
    }
}
