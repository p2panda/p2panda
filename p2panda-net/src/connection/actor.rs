// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::{Context, Result};
use iroh_gossip::proto::TopicId;
use iroh_net::endpoint::Connection;
use iroh_net::{Endpoint, NodeId};
use tokio::sync::mpsc;

use crate::connection::manager::ConnectionManager;
use crate::engine::sync::ToSyncActor;

#[derive(Debug)]
pub enum ToConnectionActor {
    /// Process a newly discovered peer and topic.
    PeerDiscovered { peer: NodeId, topic: TopicId },
    /// Initiate an outbound connection with the given peer and topic.
    Connect { peer: NodeId, topic: TopicId },
    /// Handle an inbound connection.
    Connected {
        peer: NodeId,
        connection: Connection,
    },
    /// Ask the sync engine to accept a session.
    Sync {
        peer: NodeId,
        connection: Connection,
    },
    /// Close the connection.
    Disconnect { connection: Connection },
}

/// Orchestrate connection state transitions.
///
/// The connection actor is responsible for processing connection events and invoking connection
/// manager methods.
#[derive(Debug)]
pub struct ConnectionActor {
    connection_manager: ConnectionManager,
    sync_actor_tx: mpsc::Sender<ToSyncActor>,
    inbox: mpsc::Receiver<ToConnectionActor>,
}

impl ConnectionActor {
    pub fn new(
        endpoint: Endpoint,
        inbox: mpsc::Receiver<ToConnectionActor>,
        sync_actor_tx: mpsc::Sender<ToSyncActor>,
    ) -> Self {
        let (connection_actor_tx, inbox) = mpsc::channel::<ToConnectionActor>(256);
        let connection_manager = ConnectionManager::new(endpoint, connection_actor_tx);

        Self {
            connection_manager,
            inbox,
            sync_actor_tx,
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
            ToConnectionActor::PeerDiscovered { peer, topic } => {
                self.handle_peer_discovered(peer, topic)
            }
            ToConnectionActor::Connect { peer, topic } => self.handle_connect(peer, topic).await?,
            ToConnectionActor::Connected { peer, connection } => {
                self.handle_connected(peer, connection).await?
            }
            ToConnectionActor::Sync { peer, connection } => {
                self.handle_sync(peer, connection).await?
            }
            ToConnectionActor::Disconnect { connection } => self.handle_disconnect(connection)?,
        }

        Ok(true)
    }

    fn handle_peer_discovered(&mut self, peer: NodeId, topic: TopicId) {
        self.connection_manager.add_peer(peer, topic);
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

    async fn handle_connected(&mut self, peer: NodeId, connection: Connection) -> Result<()> {
        self.connection_manager
            .accept_connection(peer, connection)
            .await?;

        Ok(())
    }

    async fn handle_sync(&self, peer: NodeId, connection: Connection) -> Result<()> {
        self.sync_actor_tx
            .send(ToSyncActor::Accept { peer, connection })
            .await?;

        Ok(())
    }

    fn handle_disconnect(&mut self, connection: Connection) -> Result<()> {
        self.connection_manager.disconnect(connection)?;

        Ok(())
    }
}
