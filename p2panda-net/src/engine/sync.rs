// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::Arc;

use anyhow::{Context, Result};
use futures_util::SinkExt;
use iroh_gossip::proto::TopicId;
use iroh_net::key::PublicKey;
// @TODO: See if we can remove the `iroh_quinn` dependency.
// @NOTE: I don't _think_ we can because we need access to the "futures-io" feature which iroh_net
// dosen't expose.
use iroh_quinn::{RecvStream, SendStream};
use p2panda_sync::traits::SyncProtocol;
use p2panda_sync::SyncError;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::PollSender;
use tracing::{debug, error};

use super::engine::ToEngineActor;

pub enum ToSyncActor {
    Sync {
        peer: PublicKey,
        topic: TopicId,
        send: SendStream,
        recv: RecvStream,
        result_tx: oneshot::Sender<Result<()>>,
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
            ToSyncActor::Shutdown => return Ok(false),
        };

        Ok(true)
    }

    async fn on_sync_message(
        &self,
        peer: PublicKey,
        topic: TopicId,
        send: SendStream,
        recv: RecvStream,
        result_tx: oneshot::Sender<Result<()>>,
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
                .run(
                    topic.as_bytes(),
                    Box::new(send),
                    Box::new(recv),
                    Box::new(sink),
                )
                .await
                .map_err(|e| anyhow::anyhow!(e));
            result_tx.send(result).expect("sync result channel closed");
        });

        // Spawn another task which picks up any new application messages and sends them
        // on to the engine for handling.
        let engine_actor_tx = self.engine_actor_tx.clone();
        tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
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
