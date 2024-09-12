// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::Arc;

use anyhow::{Context, Result};
use futures_util::SinkExt;
use iroh_gossip::proto::TopicId;
use iroh_net::key::PublicKey;
use iroh_quinn::Connection;
use p2panda_sync::traits::{AppMessage, SyncProtocol};
use p2panda_sync::SyncError;
use tokio::sync::mpsc;
use tokio_util::sync::PollSender;
use tracing::{debug, error};

use super::engine::ToEngineActor;

pub enum ToSyncActor {
    Open {
        peer: PublicKey,
        topic: TopicId,
        connection: Connection,
    },
    Accept {
        peer: PublicKey,
        connection: Connection,
    },
    Shutdown,
}

pub struct SyncActor {
    inbox: mpsc::Receiver<ToSyncActor>,
    sync_protocol: Arc<dyn SyncProtocol + 'static>,
    engine_actor_tx: mpsc::Sender<ToEngineActor>,
}

impl SyncActor {
    pub fn new(
        inbox: mpsc::Receiver<ToSyncActor>,
        sync_protocol: Arc<dyn SyncProtocol + 'static>,
        engine_actor_tx: mpsc::Sender<ToEngineActor>,
    ) -> Self {
        Self {
            inbox,
            sync_protocol,
            engine_actor_tx,
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

    async fn on_actor_message(&mut self, msg: ToSyncActor) -> Result<bool> {
        match msg {
            ToSyncActor::Open {
                peer,
                topic,
                connection,
            } => {
                self.on_open_sync(peer, topic, connection).await?;
            }
            ToSyncActor::Accept { peer, connection } => {
                self.on_accept_sync(peer, connection).await?
            }
            ToSyncActor::Shutdown => return Ok(false),
        };

        Ok(true)
    }

    /// Initiate a sync protocol session over a new bi-directional stream on the provided connections.
    async fn on_open_sync(
        &self,
        peer: PublicKey,
        topic: TopicId,
        connection: Connection,
    ) -> Result<()> {
        debug!(
            "Initiate sync session with peer {} over topic {:?}",
            peer, topic
        );

        // Set up a channel for receiving new application messages.
        let (tx, mut rx) = mpsc::channel(128);
        let sink = PollSender::new(tx).sink_map_err(|e| SyncError::Protocol(e.to_string()));

        // Spawn a task which opens a bi-directional stream over the provided connection and runs
        // the sync protocol.
        let protocol = self.sync_protocol.clone();
        tokio::spawn(async move {
            let result = async {
                let (send, recv) = connection
                    .open_bi()
                    .await
                    .map_err(|e| SyncError::Protocol(e.to_string()))?;

                protocol
                    .open(
                        topic.as_bytes(),
                        Box::new(send),
                        Box::new(recv),
                        Box::new(sink),
                    )
                    .await
            }
            .await;

            if let Err(err) = result {
                error!("{err}");
            };
        });

        // Spawn another task which picks up any new application messages and sends them
        // on to the engine for handling.
        let engine_actor_tx = self.engine_actor_tx.clone();
        tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                if let AppMessage::Topic(_) = &message {
                    error!("expected bytes from app message channel");
                    return;
                };

                if let Err(err) = engine_actor_tx
                    .send(ToEngineActor::SyncMessage {
                        message,
                        delivered_from: peer,
                        topic,
                    })
                    .await
                {
                    error!("error in sync actor: {}", err)
                };
            }
            engine_actor_tx
                .send(ToEngineActor::SyncDone { peer, topic })
                .await
                .expect("engine channel closed");
        });

        Ok(())
    }

    /// Accept a sync protocol session over a new bi-directional stream on the provided connections.
    async fn on_accept_sync(&self, peer: PublicKey, connection: Connection) -> Result<()> {
        debug!("Accept sync session with peer {}", peer);

        // Set up a channel for receiving new application messages.
        let (tx, mut rx) = mpsc::channel(128);
        let sink = PollSender::new(tx).sink_map_err(|e| SyncError::Protocol(e.to_string()));

        // Spawn a task which runs the sync protocol.
        let protocol = self.sync_protocol.clone();
        tokio::spawn(async move {
            let result = async {
                let (send, recv) = connection
                    .accept_bi()
                    .await
                    .map_err(|e| SyncError::Protocol(e.to_string()))?;

                protocol
                    .accept(Box::new(send), Box::new(recv), Box::new(sink))
                    .await
            }
            .await;

            if let Err(err) = result {
                error!("{err}");
            }
        });

        // Spawn another task which picks up any new application messages and sends them
        // on to the engine for handling.
        let engine_actor_tx = self.engine_actor_tx.clone();
        tokio::spawn(async move {
            let mut topic = None;
            while let Some(message) = rx.recv().await {
                if let AppMessage::Topic(id) = &message {
                    topic = Some(id.to_owned());
                }

                let Some(topic_id) = topic else {
                    error!("topic id not received");
                    return;
                };

                if let Err(err) = engine_actor_tx
                    .send(ToEngineActor::SyncMessage {
                        message,
                        delivered_from: peer,
                        topic: topic_id.into(),
                    })
                    .await
                {
                    error!("error in sync actor: {}", err)
                };
            }

            // If topic was never set we didn't receive any messages and so the engine was not
            // informed it should buffer messages and we can return here.
            let Some(topic) = topic else {
                return;
            };

            engine_actor_tx
                .send(ToEngineActor::SyncDone {
                    peer,
                    topic: topic.into(),
                })
                .await
                .expect("engine channel closed");
        });

        Ok(())
    }
}
