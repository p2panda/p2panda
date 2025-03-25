// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_sync::TopicQuery;
use tokio::sync::mpsc;

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

    // @TODO(glyph): Probably want to implement `recv()`, `recv_many()` and `try_recv()`.

    async fn recv(&mut self) -> Option<FromNetwork> {
        self.from_network_rx.recv().await
    }
}

impl<T> Drop for TopicStreamReceiver<T> {
    fn drop(&mut self) {
        todo!()

        // self.engine_actor_tx.send(ToEngineActor::UnsubscribeTopic { .. })
    }
}
