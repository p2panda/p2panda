// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::Arc;

use anyhow::{Context, Result};
use futures_util::SinkExt;
use iroh_gossip::proto::TopicId;
use iroh_net::key::PublicKey;
use iroh_quinn::{RecvStream, SendStream};
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
        send: SendStream,
        recv: RecvStream,
    },
    Accept {
        peer: PublicKey,
        send: SendStream,
        recv: RecvStream,
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
                send,
                recv,
            } => {
                self.on_open_sync(peer, topic, send, recv).await?;
            }
            ToSyncActor::Accept { peer, send, recv } => {
                self.on_accept_sync(peer, send, recv).await?
            }
            ToSyncActor::Shutdown => return Ok(false),
        };

        Ok(true)
    }

    async fn on_open_sync(
        &self,
        peer: PublicKey,
        topic: TopicId,
        send: SendStream,
        recv: RecvStream,
    ) -> Result<()> {
        debug!(
            "Initiate sync session with peer {} over topic {:?}",
            peer, topic
        );

        // Set up a channel for receiving new application messages.
        let (tx, mut rx) = mpsc::channel(128);
        let sink = PollSender::new(tx).sink_map_err(|e| SyncError::Protocol(e.to_string()));

        // Spawn a task which runs the sync protocol.
        let protocol = self.sync_protocol.clone();
        tokio::spawn(async move {
            let result = protocol
                .open(
                    topic.as_bytes(),
                    Box::new(send),
                    Box::new(recv),
                    Box::new(sink),
                )
                .await;

            if let Err(err) = result {
                error!("{err}");
            }
        });

        // Spawn another task which picks up any new application messages and sends them
        // on to the engine for handling.
        let engine_actor_tx = self.engine_actor_tx.clone();
        tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                let AppMessage::Bytes(bytes) = message else {
                    error!("expected bytes from app message channel");
                    return;
                };

                if let Err(err) = engine_actor_tx
                    .send(ToEngineActor::SyncMessage {
                        bytes,
                        delivered_from: peer,
                        topic,
                    })
                    .await
                {
                    error!("error in sync actor: {}", err)
                };
            }
        });

        Ok(())
    }

    async fn on_accept_sync(
        &self,
        peer: PublicKey,
        send: SendStream,
        recv: RecvStream,
    ) -> Result<()> {
        debug!("Accept sync session with peer {}", peer);

        // Set up a channel for receiving new application messages.
        let (tx, mut rx) = mpsc::channel(128);
        let sink = PollSender::new(tx).sink_map_err(|e| SyncError::Protocol(e.to_string()));

        // Spawn a task which runs the sync protocol.
        let protocol = self.sync_protocol.clone();
        tokio::spawn(async move {
            let result = protocol
                .accept(Box::new(send), Box::new(recv), Box::new(sink))
                .await;

            if let Err(err) = result {
                error!("{err}");
            }
        });

        // Spawn another task which picks up any new application messages and sends them
        // on to the engine for handling.
        let engine_actor_tx = self.engine_actor_tx.clone();
        tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                let AppMessage::Topic(topic) = message else {
                    error!("expected topic id from app message channel");
                    return;
                };

                let AppMessage::Bytes(bytes) = message else {
                    error!("expected bytes from app message channel");
                    return;
                };
                if let Err(err) = engine_actor_tx
                    .send(ToEngineActor::SyncMessage {
                        bytes,
                        delivered_from: peer,
                        topic: topic.into(),
                    })
                    .await
                {
                    error!("error in sync actor: {}", err)
                };
            }
        });

        Ok(())
    }
}
