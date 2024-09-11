// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use futures_util::SinkExt;
use iroh_gossip::proto::TopicId;
use iroh_net::key::PublicKey;
use iroh_quinn::{RecvStream, SendStream};
use p2panda_sync::traits::{AppMessage, SyncProtocol};
use p2panda_sync::SyncError;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::PollSender;
use tracing::{debug, error};

use super::engine::ToEngineActor;

pub enum ToSyncActor {
    Open {
        peer: PublicKey,
        topic: TopicId,
        send: SendStream,
        recv: RecvStream,
        result_tx: oneshot::Sender<Result<(), SyncError>>,
    },
    Accept {
        topic: TopicId,
        send: SendStream,
        recv: RecvStream,
        result_tx: oneshot::Sender<Result<(), SyncError>>,
    },
}

#[derive(Clone, Default)]
pub struct SyncProtocolMap(HashMap<TopicId, Arc<dyn SyncProtocol>>);

impl SyncProtocolMap {
    pub fn add(&mut self, topic: TopicId, handler: impl SyncProtocol + 'static) {
        self.0.insert(topic, Arc::new(handler));
    }

    pub fn get(&self, topic: TopicId) -> Option<&Arc<dyn SyncProtocol + 'static>> {
        self.0.get(&topic)
    }
}
impl std::fmt::Debug for SyncProtocolMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SyncProtocolMap").finish()
    }
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
                result_tx,
            } => {
                self.on_open_sync(peer, topic, send, recv, result_tx)
                    .await?;
            }
            ToSyncActor::Accept {
                topic,
                send,
                recv,
                result_tx,
            } => todo!(),
        };

        Ok(true)
    }

    async fn on_open_sync(
        &self,
        peer: PublicKey,
        topic: TopicId,
        send: SendStream,
        recv: RecvStream,
        result_tx: oneshot::Sender<Result<(), SyncError>>,
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
            result_tx.send(result).expect("sync result channel closed");
        });

        // Spawn another task which picks up any new application messages and sends them
        // on to the engine for handling.
        let engine_actor_tx = self.engine_actor_tx.clone();
        tokio::spawn(async move {
            while let Some(message) = rx.blocking_recv() {
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
        result_tx: oneshot::Sender<Result<(), SyncError>>,
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
            result_tx.send(result).expect("sync result channel closed");
        });

        // Spawn another task which picks up any new application messages and sends them
        // on to the engine for handling.
        let engine_actor_tx = self.engine_actor_tx.clone();
        tokio::spawn(async move {
            while let Some(message) = rx.blocking_recv() {
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
