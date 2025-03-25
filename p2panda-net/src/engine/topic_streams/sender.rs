// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_sync::TopicQuery;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::{SendError, TrySendError};

use crate::engine::engine::ToEngineActor;
use crate::network::ToNetwork;
use crate::TopicId;

// @TODO(glyph): the TopicStreams struct is where we keep the reference counters for the stream
// subscribers.

// @TODO(glyph): Docs.
#[derive(Debug)]
pub struct TopicStreamSender<T> {
    topic: T,
    stream_id: usize,
    to_network_tx: mpsc::Sender<ToNetwork>,
    engine_actor_tx: mpsc::Sender<ToEngineActor<T>>,
}

impl<T> TopicStreamSender<T>
where
    T: TopicQuery + TopicId + 'static,
{
    pub(crate) async fn new(
        topic: T,
        stream_id: usize,
        to_network_tx: mpsc::Sender<ToNetwork>,
        engine_actor_tx: mpsc::Sender<ToEngineActor<T>>,
    ) -> Self {
        Self {
            topic,
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
        todo!()

        // self.engine_actor_tx.send(ToEngineActor::UnsubscribeTopic { .. })
    }
}
