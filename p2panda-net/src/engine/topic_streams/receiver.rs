// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_sync::TopicQuery;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TryRecvError;

use crate::engine::engine::ToEngineActor;
use crate::network::FromNetwork;
use crate::TopicId;

// @TODO(glyph): Docs.
#[derive(Debug)]
pub struct TopicStreamReceiver<T> {
    topic: T,
    stream_id: usize,
    from_network_rx: mpsc::Receiver<FromNetwork>,
    engine_actor_tx: mpsc::Sender<ToEngineActor<T>>,
}

impl<T> TopicStreamReceiver<T>
where
    T: TopicQuery + TopicId + 'static,
{
    pub(crate) async fn new(
        topic: T,
        stream_id: usize,
        from_network_rx: mpsc::Receiver<FromNetwork>,
        engine_actor_tx: mpsc::Sender<ToEngineActor<T>>,
    ) -> Self {
        Self {
            topic,
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
        todo!()

        // self.engine_actor_tx.send(ToEngineActor::UnsubscribeTopic { .. })
    }
}
