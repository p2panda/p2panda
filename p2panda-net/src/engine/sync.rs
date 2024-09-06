// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::{Context, Result};
use futures_lite::{AsyncRead, AsyncWrite};
use iroh_gossip::proto::TopicId;
use iroh_net::key::PublicKey;
use p2panda_sync::traits::{SyncEngine, SyncProtocol};
use p2panda_sync::{Engine, SyncError};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};
use tracing::debug;

use crate::engine::ToEngineActor;

pub enum ToSyncActor {
    Sync {
        peer: PublicKey,
        topic: TopicId,
        tx: Box<dyn AsyncWrite + Send + Unpin>,
        rx: Box<dyn AsyncRead + Send + Unpin>,
        result_tx: oneshot::Sender<Result<(), SyncError>>,
    },
}

pub struct SyncActor<P> {
    engine_actor_tx: mpsc::Sender<ToEngineActor>,
    inbox: mpsc::Receiver<ToSyncActor>,
    sync_engine: Engine<P>,
}

impl<P> SyncActor<P>
where
    P: Clone + SyncProtocol<Topic = TopicId> + 'static,
    for<'a> P::Message: Serialize + Deserialize<'a> + Send + 'static,
{
    pub fn new(
        inbox: mpsc::Receiver<ToSyncActor>,
        engine_actor_tx: mpsc::Sender<ToEngineActor>,
        protocol: P,
    ) -> Self {
        let sync_engine = Engine::new(protocol);
        Self {
            engine_actor_tx,
            inbox,
            sync_engine,
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
