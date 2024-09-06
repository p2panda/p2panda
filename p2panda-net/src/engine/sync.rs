// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Display;

use anyhow::{Context, Result};
use futures_lite::{AsyncRead, AsyncWrite};
use iroh_gossip::proto::TopicId;
use iroh_net::key::PublicKey;
use p2panda_sync::traits::{SyncEngine, SyncProtocol};
use p2panda_sync::{Engine, SyncError};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};
use tracing::debug;

pub enum ToSyncActor<T> {
    Sync {
        peer: PublicKey,
        topic: T,
        tx: Box<dyn AsyncWrite + Send + Unpin>,
        rx: Box<dyn AsyncRead + Send + Unpin>,
        result_tx: oneshot::Sender<Result<(), SyncError>>,
    },
}

pub struct SyncActor<P>
where
    P::Topic: std::fmt::Debug,
    P: SyncProtocol,
{
    inbox: mpsc::Receiver<ToSyncActor<<P as SyncProtocol>::Topic>>,
    // engine_actor_tx: mpsc::Sender<ToEngineActor>,
    sync_engine: Engine<P>,
}

impl<P> SyncActor<P>
where
    P::Topic: std::fmt::Debug + Display + Send,
    P: Clone + SyncProtocol + 'static,
    for<'a> P::Message: Serialize + Deserialize<'a> + Send + 'static,
{
    pub fn new(inbox: mpsc::Receiver<ToSyncActor<P::Topic>>, protocol: P) -> Self {
        let sync_engine = Engine::new(protocol);
        Self { inbox, sync_engine }
    }

    pub async fn run(&mut self) -> Result<()> {
        loop {
            tokio::select! {
                msg = self.inbox.recv() => {
                    let msg = msg.context("inbox closed")?;
                    if !self.on_actor_message(msg).await.context("on_actor_message")? {
                        break;
                    }
                },
            }
        }

        Ok(())
    }

    async fn on_actor_message(&mut self, msg: ToSyncActor<P::Topic>) -> Result<bool> {
        match msg {
            ToSyncActor::Sync {
                peer,
                topic,
                tx,
                rx,
                result_tx,
            } => {
                debug!(
                    "Initiate sync session with peer {} over topic {}",
                    peer, topic
                );
                let session = self.sync_engine.session(tx, rx);
                tokio::spawn(async move {
                    let result = session.run(topic).await;
                    result_tx.send(result).expect("sync result message closed");
                });
            }
        }

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use futures_util::{Sink, SinkExt, Stream, StreamExt};
    use iroh_net::key::SecretKey;
    use p2panda_sync::traits::SyncProtocol;
    use p2panda_sync::SyncError;
    use serde::{Deserialize, Serialize};
    use tokio::sync::{mpsc, oneshot};
    use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

    use crate::engine::sync::{SyncActor, ToSyncActor};

    const TOPIC_ID: &str = "ping_pong";

    // The protocol message types.
    #[derive(Serialize, Deserialize)]
    enum Message {
        Ping,
        Pong,
    }

    // Ping pong protocol.
    #[derive(Clone, Default)]
    struct MyProtocol {
        sent_ping: bool,
        sent_pong: bool,
        received_ping: bool,
        received_pong: bool,
    }

    impl SyncProtocol for MyProtocol {
        type Topic = &'static str;
        type Message = Message;

        async fn run(
            mut self,
            topic: Self::Topic,
            mut sink: impl Sink<Message, Error = SyncError> + Unpin,
            mut stream: impl Stream<Item = Result<Message, SyncError>> + Unpin,
        ) -> Result<(), SyncError> {
            if topic != TOPIC_ID {
                return Err(SyncError::Protocol("we only ping and pong".to_string()));
            }

            sink.send(Message::Ping).await?;
            self.sent_ping = true;

            while let Some(result) = stream.next().await {
                let message = result?;

                match message {
                    Message::Ping => {
                        self.received_ping = true;
                        sink.send(Message::Pong).await?;
                        self.sent_pong;
                    }
                    Message::Pong => {
                        self.received_pong = true;
                        break;
                    }
                }
            }

            Ok(())
        }
    }

    #[tokio::test]
    async fn ping_protocol_test() {
        let (tx_a, rx_a) = mpsc::channel(128);
        let (tx_b, rx_b) = mpsc::channel(128);

        let (tx_a_result, rx_a_result) = oneshot::channel();
        let (tx_b_result, rx_b_result) = oneshot::channel();

        let mut sync_actor_a = SyncActor::new(rx_a, MyProtocol::default());
        let mut sync_actor_b = SyncActor::new(rx_b, MyProtocol::default());

        tokio::spawn(async move { sync_actor_a.run().await });
        tokio::spawn(async move { sync_actor_b.run().await });

        let (peer_a, peer_b) = tokio::io::duplex(64 * 1024);
        let (peer_a_read, peer_a_write) = tokio::io::split(peer_a);
        let (peer_b_read, peer_b_write) = tokio::io::split(peer_b);

        tx_a.send(ToSyncActor::Sync {
            peer: SecretKey::generate().public(),
            topic: TOPIC_ID,
            tx: Box::new(peer_a_write.compat_write()),
            rx: Box::new(peer_a_read.compat()),
            result_tx: tx_a_result,
        })
        .await
        .unwrap();

        tx_b.send(ToSyncActor::Sync {
            peer: SecretKey::generate().public(),
            topic: TOPIC_ID,
            tx: Box::new(peer_b_write.compat_write()),
            rx: Box::new(peer_b_read.compat()),
            result_tx: tx_b_result,
        })
        .await
        .unwrap();

        let (result1, result2) = tokio::join!(rx_a_result, rx_b_result);

        assert!(result1.is_ok());
        assert!(result2.is_ok());
    }
}
