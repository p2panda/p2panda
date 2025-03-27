// SPDX-License-Identifier: MIT OR Apache-2.0

use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TryRecvError;
use tracing::warn;

use crate::engine::engine::ToEngineActor;
use crate::engine::topic_streams::TopicStreamChannel;
use crate::network::FromNetwork;

// @TODO(glyph): Docs.
#[derive(Debug)]
pub struct TopicStreamReceiver<T> {
    topic: Option<T>,
    stream_id: usize,
    from_network_rx: mpsc::Receiver<FromNetwork>,
    engine_actor_tx: mpsc::Sender<ToEngineActor<T>>,
}

impl<T> TopicStreamReceiver<T> {
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
}

impl<T> Drop for TopicStreamReceiver<T> {
    fn drop(&mut self) {
        if let Some(topic) = self.topic.take() {
            if let Err(_) = self
                .engine_actor_tx
                .blocking_send(ToEngineActor::UnsubscribeTopic {
                    topic,
                    stream_id: self.stream_id,
                    channel_type: TopicStreamChannel::Receiver,
                })
            {
                warn!("engine actor receiver dropped before topic unsubscribe event could be sent")
            }
        }
    }
}
