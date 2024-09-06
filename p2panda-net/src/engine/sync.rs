// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Display;
use std::marker::PhantomData;

use anyhow::{Context, Result};
use iroh_net::key::PublicKey;
use iroh_quinn::{RecvStream, SendStream};
use p2panda_sync::traits::{SyncEngine, SyncProtocol};
use p2panda_sync::SyncError;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};
use tracing::debug;

use crate::TopicId;

pub enum ToSyncActor<T>
{
    Sync {
        peer: PublicKey,
        gossip_topic: TopicId,
        sync_topic: T,
        send: SendStream,
        recv: RecvStream,
        live_message_channel: mpsc::Sender<Vec<u8>>,
        result_tx: oneshot::Sender<Result<(), SyncError>>,
    },
}

pub struct SyncActor<P, E>
where
    P::Topic: std::fmt::Debug,
    P: SyncProtocol<Context = mpsc::Sender<Vec<u8>>>,
    E: SyncEngine<P, Box<SendStream>, Box<RecvStream>> + 'static,
{
    inbox: mpsc::Receiver<ToSyncActor<<P as SyncProtocol>::Topic>>,
    // engine_actor_tx: mpsc::Sender<ToEngineActor>,
    protocol: P,
    phantom: PhantomData<E>,
}

impl<P, E> SyncActor<P, E>
where
    P::Topic: std::fmt::Debug + Display + Send,
    P: Clone + SyncProtocol<Context = mpsc::Sender<Vec<u8>>> + 'static,
    for<'a> P::Message: Serialize + Deserialize<'a> + Send,
    E: SyncEngine<P, Box<SendStream>, Box<RecvStream>>,
{
    pub fn new(
        inbox: mpsc::Receiver<ToSyncActor<P::Topic>>,
        protocol: P,
    ) -> Self {
        Self {
            inbox,
            protocol,
            phantom: PhantomData::default(),
        }
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
                gossip_topic,
                sync_topic,
                send,
                recv,
                live_message_channel,
                result_tx,
            } => {
                debug!(
                    "Initiate sync session with peer {} over topic {}",
                    peer, sync_topic
                );
                let session = E::session(self.protocol.clone(), Box::new(send), Box::new(recv));
                tokio::spawn(async move {
                    let result = session.run(sync_topic, live_message_channel).await;
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
    use p2panda_sync::traits::SyncProtocol;
    use p2panda_sync::SyncError;
    use serde::{Deserialize, Serialize};
    use tokio::sync::mpsc;

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
        type Context = mpsc::Sender<String>;

        async fn run(
            mut self,
            topic: Self::Topic,
            mut sink: impl Sink<Message, Error = SyncError> + Unpin,
            mut stream: impl Stream<Item = Result<Message, SyncError>> + Unpin,
            context: mpsc::Sender<String>,
        ) -> Result<(), SyncError> {
            if topic != TOPIC_ID {
                return Err(SyncError::Protocol("we only ping and pong".to_string()));
            }

            self.sent_ping = true;

            while let Some(result) = stream.next().await {
                let message = result?;

                match message {
                    Message::Ping => {
                        // tx.send("PING".to_string()).await.expect("channel closed");
                        self.received_ping = true;
                        sink.send(Message::Pong).await?;
                        self.sent_pong;
                    }
                    Message::Pong => {
                        // tx.send("PONG".to_string()).await.expect("channel closed");
                        self.received_pong = true;
                        break;
                    }
                }
            }

            Ok(())
        }
    }
}
