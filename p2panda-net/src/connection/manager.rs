// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashSet;

use anyhow::Result;
use iroh_net::endpoint::Connection;
use iroh_net::{Endpoint, NodeId};
use tokio::sync::mpsc::Sender;

use crate::connection::ToConnectionActor;
use crate::sync_connection::SYNC_CONNECTION_ALPN;

#[derive(Debug)]
pub struct ConnectionManager {
    completed_sync_sessions: HashSet<NodeId>,
    connection_actor_tx: Sender<ToConnectionActor>,
    endpoint: Endpoint,
}

impl ConnectionManager {
    pub fn new(endpoint: Endpoint, connection_actor_tx: Sender<ToConnectionActor>) -> Self {
        Self {
            completed_sync_sessions: HashSet::new(),
            connection_actor_tx,
            endpoint,
        }
    }

    /// Attempt to connect with the given peer.
    ///
    /// A `None` value will be returned if a connection has already been established and is
    /// currently active.
    pub async fn connect(&mut self, peer: NodeId) -> Result<Option<Connection>> {
        let connection = self
            .endpoint
            .connect_by_node_id(peer, SYNC_CONNECTION_ALPN)
            .await?;

        Ok(Some(connection))
    }

    /// Accept an inbound connection if an active connection does not currently exist for the given
    /// peer and then begin the sync handshake if a previous session has not already been
    /// successfully completed.
    pub async fn accept_connection(&mut self, peer: NodeId, connection: Connection) -> Result<()> {
        self.connection_actor_tx
            .send(ToConnectionActor::Sync { peer, connection })
            .await?;

        Ok(())
    }

    /// Log sync as completed for the given peer.
    fn complete_sync(&mut self, peer: NodeId) -> bool {
        self.completed_sync_sessions.insert(peer)
    }

    /// Query the sync state of the given peer.
    pub fn sync_completed(&self, peer: &NodeId) -> bool {
        self.completed_sync_sessions.contains(peer)
    }
}
