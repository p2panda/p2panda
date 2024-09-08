// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use futures_util::SinkExt;
use iroh_net::key::PublicKey;
use iroh_quinn::{RecvStream, SendStream};
use p2panda_sync::traits::SyncProtocol;
use p2panda_sync::SyncError;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::PollSender;
use tracing::debug;

use crate::TopicId;

use super::engine::ToEngineActor;

pub enum ToSyncActor {
    Sync {
        peer: PublicKey,
        protocol: &'static str,
        topic: TopicId,
        send: SendStream,
        recv: RecvStream,
        live_message_channel: mpsc::Sender<Vec<u8>>,
        result_tx: oneshot::Sender<Result<(), SyncError>>,
    },
}

type ProtocolMap = HashMap<&'static str, Arc<dyn SyncProtocol>>;

pub struct SyncActor {
    inbox: mpsc::Receiver<ToSyncActor>,
    protocol_map: ProtocolMap,
}

impl SyncActor {
    pub fn new(inbox: mpsc::Receiver<ToSyncActor>, protocol_map: ProtocolMap) -> Self {
        Self {
            inbox,
            protocol_map,
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
                protocol,
                topic,
                send,
                recv,
                live_message_channel,
                result_tx,
            } => {
                debug!(
                    "Initiate sync session with peer {} over topic {:?}",
                    peer, topic
                );
                let protocol = self
                    .protocol_map
                    .get(protocol)
                    .expect("unknown protocol")
                    .clone();

                let sink = PollSender::new(live_message_channel)
                    .sink_map_err(|e| SyncError::Protocol(e.to_string()));

                tokio::spawn(async move {
                    let result = protocol
                        .run(Box::new(send), Box::new(recv), Box::new(sink))
                        .await;
                    result_tx.send(result).expect("sync result message closed");
                });
            }
        }

        Ok(true)
    }
}
