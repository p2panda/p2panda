// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::{Context, Result};
use iroh_gossip::proto::TopicId;
use iroh_net::endpoint::Connection;
use iroh_net::{Endpoint, NodeId};
use tokio::sync::mpsc;
use tracing::{debug, error};

use crate::connection::manager::ConnectionManager;
use crate::engine::sync::ToSyncActor;

#[derive(Debug)]
/// Connection events.
pub enum ToConnectionActor {
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
    /// Log successfully sync session.
    SyncComplete { peer: NodeId, topic: TopicId },
    /// Terminate the actor.
    Shutdown,
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
    connection_actor_tx: mpsc::Sender<ToConnectionActor>,
}

impl ConnectionActor {
    pub fn new(endpoint: Endpoint, sync_actor_tx: mpsc::Sender<ToSyncActor>) -> Self {
        let (connection_actor_tx, inbox) = mpsc::channel(256);
        let connection_manager = ConnectionManager::new(endpoint, connection_actor_tx.clone());

        Self {
            connection_manager,
            inbox,
            connection_actor_tx,
            sync_actor_tx,
        }
    }

    pub fn sender(&self) -> mpsc::Sender<ToConnectionActor> {
        self.connection_actor_tx.clone()
    }

    pub async fn run(&mut self) -> Result<()> {
        debug!("running connection actor!");

        loop {
            tokio::select! {
                msg = self.inbox.recv() => {
                    let msg = msg.context("inbox closed")?;
                    match self.on_actor_message(msg).await {
                        Err(err) => error!("failed to process connection event: {err:?}"),
                        Ok(result) => {
                            if !result {
                                break
                            }
                        }
                    }
                },
            }
        }

        Ok(())
    }

    async fn on_actor_message(&mut self, msg: ToConnectionActor) -> Result<bool> {
        // @TODO: Consider implementing Display for nicer logging of `ToConnectionActor` events.
        debug!("connection event: {msg:?}");

        match msg {
            ToConnectionActor::Connect { peer, topic } => self.handle_connect(peer, topic).await?,
            ToConnectionActor::Connected { peer, connection } => {
                self.handle_connected(peer, connection).await?
            }
            ToConnectionActor::Sync { peer, connection } => {
                self.handle_sync(peer, connection).await?
            }
            ToConnectionActor::SyncComplete { peer, topic } => {
                self.handle_sync_complete(peer, topic).await?
            }
            ToConnectionActor::Shutdown => return Ok(false),
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

    async fn handle_sync_complete(&self, peer: NodeId, topic: TopicId) -> Result<()> {
        todo!()
    }
}
