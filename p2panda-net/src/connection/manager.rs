// SPDX-License-Identifier: AGPL-3.0-or-later

// Connection manager.
//
// Minimal functionality for first-pass:
//
// - Maintain an address book
//   - Update upon discovery of new peers
// - Connect to new peers
// - Handle inbound peer connections
// - Invoke sync sessions
// - Disconnect cleanly
//
// Second-pass features:
//
// - Retry failed connection attempts
//   - Implement cool-down for recurrent failures
// - Ensure maximum concurrent connection limit is respected

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use iroh_gossip::proto::TopicId;
use iroh_net::endpoint::{self, Connection};
use iroh_net::{Endpoint, NodeId};
use tokio::sync::mpsc::Sender;
use tracing::{debug, warn};

use crate::connection::ToConnectionActor;
use crate::sync_connection::SYNC_CONNECTION_ALPN;

// @TODO: Look at `PeerMap` in `src/engine/engine.rs`
// That contains some address book functionality.
// Be sure we're not duplicating efforts.

#[derive(Debug)]
pub struct ConnectionManager {
    active_connections: HashSet<NodeId>,
    address_book: HashMap<NodeId, TopicId>,
    completed_sync_sessions: HashSet<NodeId>,
    connection_actor_tx: Sender<ToConnectionActor>,
    endpoint: Endpoint,
}

impl ConnectionManager {
    pub fn new(endpoint: Endpoint, connection_actor_tx: Sender<ToConnectionActor>) -> Self {
        Self {
            active_connections: HashSet::new(),
            address_book: HashMap::new(),
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
        if self.currently_connected(&peer) {
            Ok(None)
        } else {
            let connection = self
                .endpoint
                .connect_by_node_id(peer, SYNC_CONNECTION_ALPN)
                .await?;

            self.active_connections.insert(peer);

            if let Err(err) = self.listen_for_disconnection(connection.clone()).await {
                warn!("failed to spawn disconnection listener: {err}")
            }

            Ok(Some(connection))
        }
    }

    /// Close the given connection and remove the associated peer from the set of active
    /// connections.
    pub fn disconnect(&mut self, connection: Connection) -> Result<()> {
        connection.close(0u8.into(), b"close from disconnect");

        let peer = endpoint::get_remote_node_id(&connection)?;
        self.active_connections.remove(&peer);

        Ok(())
    }

    /// Accept an inbound connection if an active connection does not currently exist for the given
    /// peer and then begin the sync handshake if a previous session has not already been
    /// successfully completed.
    pub async fn accept_connection(&mut self, peer: NodeId, connection: Connection) -> Result<()> {
        if !self.currently_connected(&peer) {
            // @TODO: I think sync completion status tracking should be the responsibility of the sync
            // engine. We should simply be passing along the message here.
            if !self.sync_completed(&peer) {
                self.activate_connection(peer);
                self.listen_for_disconnection(connection.clone()).await?;

                self.connection_actor_tx
                    .send(ToConnectionActor::Sync { peer, connection })
                    .await?;
            }
        }

        Ok(())
    }

    /// Listen for closure of the connection; this may occur due to an error or because of an
    /// action taken by the remote peer.
    pub async fn listen_for_disconnection(&self, connection: Connection) -> Result<()> {
        let connection_actor_tx = self.connection_actor_tx.clone();

        tokio::task::spawn(async move {
            let reason = connection.closed().await;
            debug!("sync connection closed: {reason}");

            if let Err(err) = connection_actor_tx
                .send(ToConnectionActor::Disconnect { connection })
                .await
            {
                warn!("connection actor sender: {err}")
            }
        })
        .await?;

        Ok(())
    }

    // @TODO: This should be removed if sync state tracking is added in the sync engine.
    /// Log sync as completed for the given peer.
    fn complete_sync(&mut self, peer: NodeId) {
        // Ignore the returned `bool`.
        let _ = self.completed_sync_sessions.insert(peer);
    }

    // @TODO: This should be removed if sync state tracking is added in the sync engine.
    /// Query the sync state of the given peer.
    pub fn sync_completed(&self, peer: &NodeId) -> bool {
        self.completed_sync_sessions.contains(peer)
    }

    /// Query the connection state of the given peer.
    pub fn currently_connected(&self, peer: &NodeId) -> bool {
        self.active_connections.contains(peer)
    }

    pub fn activate_connection(&mut self, peer: NodeId) {
        let _ = self.active_connections.insert(peer);
    }

    /// Add the given peer and topic to the address book.
    ///
    /// Attempt an outbound connection if the peer-topic combination was not already known.
    pub async fn add_peer(&mut self, peer: NodeId, topic: TopicId) -> Result<()> {
        if self.address_book.insert(peer, topic).is_none() {
            self.connection_actor_tx
                .send(ToConnectionActor::Connect { peer, topic })
                .await?;
        }

        Ok(())
    }

    fn remove_peer(&mut self, peer: &NodeId) {
        self.address_book.remove(peer);
    }
}
