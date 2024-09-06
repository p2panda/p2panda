// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::{Context, Result};
use futures_lite::{AsyncRead, AsyncWrite};
use iroh_gossip::proto::TopicId;
use iroh_net::key::PublicKey;
use tokio::sync::mpsc;
use tracing::{error, warn};

use crate::engine::ToEngineActor;

pub enum ToSyncActor {
    Sync {
        peer: PublicKey,
        topic: TopicId,
        tx: Box<dyn AsyncWrite>,
        rx: Box<dyn AsyncRead>,
    },
}

pub struct SyncActor {
    engine_actor_tx: mpsc::Sender<ToEngineActor>,
    inbox: mpsc::Receiver<ToSyncActor>,
}

impl SyncActor {
    pub fn new(
        inbox: mpsc::Receiver<ToSyncActor>,
        engine_actor_tx: mpsc::Sender<ToEngineActor>,
    ) -> Self {
        Self {
            engine_actor_tx,
            inbox,
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
                tx,
                rx,
            } => todo!(),
        }

        Ok(true)
    }
}
