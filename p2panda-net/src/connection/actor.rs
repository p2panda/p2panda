// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Display;

use anyhow::{Context, Result};
use iroh_gossip::proto::TopicId;
use iroh_net::endpoint::Connection;
use iroh_net::{Endpoint, NodeId};
use p2panda_sync::SyncError;
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
    /// Register new topics of interest for a peer.
    UpdatePeerTopics { peer: NodeId, topics: Vec<TopicId> },
    /// Ask the sync engine to accept a session.
    Sync {
        peer: NodeId,
        connection: Connection,
    },
    /// Log successful sync session.
    SyncSucceeded { peer: NodeId, topic: TopicId },
    /// Log unsuccessful sync session.
    SyncFailed {
        peer: NodeId,
        topic: TopicId,
        err: SyncError,
    },
    /// Terminate the actor.
    Shutdown,
}

impl Display for ToConnectionActor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            ToConnectionActor::Connect { peer, topic } => {
                write!(f, "connect to peer {peer} on topic {topic}")
            }
            ToConnectionActor::Connected { peer, .. } => {
                write!(f, "connected to peer {peer}")
            }
            ToConnectionActor::UpdatePeerTopics { peer, topics } => {
                write!(f, "update topics for peer {peer}: {topics:?}")
            }
            ToConnectionActor::Sync { peer, .. } => {
                write!(f, "accept sync session with peer {peer}")
            }
            ToConnectionActor::SyncSucceeded { peer, topic } => {
                write!(f, "sync succeeded with peer {peer} on topic {topic}")
            }
            ToConnectionActor::SyncFailed { peer, topic, err } => {
                write!(
                    f,
                    "sync failed with peer {peer} on topic {topic} due to {err}"
                )
            }
            ToConnectionActor::Shutdown => write!(f, "shutdown the actor"),
        }
    }
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
        debug!("{msg}");

        match msg {
            ToConnectionActor::Connected { peer, connection } => {
                self.handle_connected(peer, connection).await?
            }
            ToConnectionActor::Sync { peer, connection } => {
                self.handle_sync(peer, connection).await?
            }
            ToConnectionActor::SyncSucceeded { peer, topic } => {
                self.handle_sync_succeeded(peer, topic).await?
            }
            ToConnectionActor::SyncFailed { peer, topic, err } => {
                self.handle_sync_failed(peer, topic, err).await?
            }
            ToConnectionActor::Shutdown => return Ok(false),
            ToConnectionActor::UpdatePeerTopics { peer, topics } => {
                self.handle_update_peer(peer, topics).await?;
            }
            ToConnectionActor::Connect { peer, topic } => self.handle_connect(peer, topic).await?,
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

    async fn handle_update_peer(&mut self, peer: NodeId, topics: Vec<TopicId>) -> Result<()> {
        self.connection_manager
            .update_peer_topics(peer, topics.clone())
            .await?;

        Ok(())
    }

    async fn handle_sync(&self, peer: NodeId, connection: Connection) -> Result<()> {
        self.sync_actor_tx
            .send(ToSyncActor::Accept { peer, connection })
            .await?;

        Ok(())
    }

    async fn handle_sync_succeeded(&mut self, peer: NodeId, topic: TopicId) -> Result<()> {
        self.connection_manager
            .complete_successful_sync(peer, topic);

        Ok(())
    }

    async fn handle_sync_failed(
        &mut self,
        peer: NodeId,
        topic: TopicId,
        err: SyncError,
    ) -> Result<()> {
        self.connection_manager
            .complete_failed_sync(peer, topic, err);

        Ok(())
    }
}
