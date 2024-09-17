// Connection actor.
//

// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::{Context, Result};
use futures_lite::{AsyncRead, AsyncWrite};
use iroh_gossip::proto::TopicId;
use iroh_net::endpoint::Connection;
use iroh_net::key::PublicKey;
use iroh_net::{Endpoint, NodeId};
use tokio::sync::mpsc;
use tracing::{error, warn};

use crate::connection::manager::ConnectionManager;

#[derive(Debug)]
pub enum ToConnectionActor {
    Connect { peer: NodeId, topic: TopicId },
    Disconnect { connection: Connection },
    PeerDiscovered { peer: NodeId, topic: TopicId },
}

pub struct ConnectionActor {
    connection_manager: ConnectionManager,
    // @TODO: Add sync actor sender.
    //sync_actor_tx: mpsc::Sender<ToSyncActor>,
    inbox: mpsc::Receiver<ToConnectionActor>,
}

impl ConnectionActor {
    pub fn new(
        endpoint: Endpoint,
        inbox: mpsc::Receiver<ToConnectionActor>,
        //sync_actor_tx: mpsc::Sender<ToSyncActor>,
    ) -> Self {
        let (connection_actor_tx, inbox) = mpsc::channel::<ToConnectionActor>(256);
        let connection_manager = ConnectionManager::new(endpoint, connection_actor_tx);

        Self {
            connection_manager,
            //sync_actor_tx,
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

    async fn on_actor_message(&mut self, msg: ToConnectionActor) -> Result<bool> {
        match msg {
            ToConnectionActor::Connect { peer, topic } => self.handle_connect(peer, topic).await?,
            ToConnectionActor::Disconnect { connection } => {
                self.handle_disconnect(connection).await?
            }
            ToConnectionActor::PeerDiscovered { peer, topic } => {
                self.handle_peer_discovered(peer, topic).await?
            }
        }

        Ok(true)
    }

    async fn handle_connect(&mut self, peer: NodeId, topic: TopicId) -> Result<()> {
        if let Some(connection) = self.connection_manager.connect(peer).await? {
            self.sync_actor_tx
                .send(ToSyncActor::Open {
                    peer,
                    topic,
                    connection,
                })
                .await?;
        }

        Ok(())
    }

    async fn handle_disconnect(&mut self, connection: Connection) -> Result<()> {
        todo!()
    }

    async fn handle_peer_discovered(&mut self, peer: NodeId, topic: TopicId) -> Result<()> {
        todo!()
    }
}
