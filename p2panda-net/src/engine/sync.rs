// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use futures_util::SinkExt;
use iroh_gossip::proto::TopicId;
use iroh_net::key::PublicKey;
use iroh_quinn::{RecvStream, SendStream};
use p2panda_sync::traits::SyncProtocol;
use p2panda_sync::SyncError;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::PollSender;
use tracing::{debug, error};

use super::engine::ToEngineActor;

pub enum ToSyncActor {
    RegisterHandler {
        topic: TopicId,
        handler: Arc<dyn SyncProtocol + 'static>,
    },
    Sync {
        peer: PublicKey,
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
    handlers: SyncProtocolMap,
    engine_actor_tx: mpsc::Sender<ToEngineActor>,
}

impl SyncActor {
    pub fn new(
        inbox: mpsc::Receiver<ToSyncActor>,
        handlers: SyncProtocolMap,
        engine_actor_tx: mpsc::Sender<ToEngineActor>,
    ) -> Self {
        Self {
            inbox,
            handlers,
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
            ToSyncActor::Sync {
                peer,
                topic,
                send,
                recv,
                result_tx,
            } => {
                self.on_sync_message(peer, topic, send, recv, result_tx)
                    .await?;
            }
            ToSyncActor::RegisterHandler { topic, handler } => {
                self.handlers.0.insert(topic, handler);
            }
        };

        Ok(true)
    }

    async fn on_sync_message(
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

        // Get the protocol handler for this topic.
        let Some(protocol) = self.handlers.get(topic).cloned() else {
            return Err(anyhow::anyhow!("SyncActor error: protocol not found"));
        };

        // Set up a channel for receiving new application messages.
        let (tx, mut rx) = mpsc::channel(128);
        let sink = PollSender::new(tx).sink_map_err(|e| SyncError::Protocol(e.to_string()));

        // Spawn a task which runs the sync protocol.
        tokio::spawn(async move {
            let result = protocol
                .run(Box::new(send), Box::new(recv), Box::new(sink))
                .await;
            result_tx.send(result).expect("sync result channel closed");
        });

        // Spawn another task which picks up any new application messages and sends them
        // on to the engine for handling.
        let engine_actor_tx = self.engine_actor_tx.clone();
        tokio::spawn(async move {
            while let Some(message) = rx.blocking_recv() {
                if let Err(err) = engine_actor_tx
                    .send(ToEngineActor::SyncMessage {
                        bytes: message,
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
}
