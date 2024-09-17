// Connection manager.
//
// Minimal functionality for first-pass:
//
// - Maintain an address book
//   - Update upon discovery of new peers
// - Connect to new peers
// - Handle inbound peer connections
// - Invoke sync sessions
// - Record successful sync sessions
//   - I think...this may be a concern of the sync actor
// - Disconnect cleanly
//
// Second-pass features:
//
// - Retry failed connection attempts
//   - Implement cool-down for recurrent failures
// - Ensure maximum concurrent connection limit is respected

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::Result;
use futures_lite::future::Boxed as BoxedFuture;
use iroh_gossip::proto::TopicId;
use iroh_net::endpoint::{self, Connecting, Connection};
use iroh_net::{Endpoint, NodeId};
use tracing::{debug, debug_span, warn};

use crate::protocols::ProtocolHandler;

// @TODO: Look at `PeerMap` in `src/engine/engine.rs`
// That contains some address book functionality.
// Be sure we're not duplicating efforts.

pub const SYNC_CONNECTION_ALPN: &[u8] = b"/p2panda-net-sync/0";

#[derive(Debug)]
struct ConnectionManager {
    active_connections: HashSet<NodeId>,
    address_book: HashMap<NodeId, TopicId>,
    completed_sync_sessions: HashSet<NodeId>,
    endpoint: Endpoint,
}

impl ConnectionManager {
    pub fn new(endpoint: Endpoint) -> Self {
        Self {
            active_connections: HashSet::new(),
            address_book: HashMap::new(),
            completed_sync_sessions: HashSet::new(),
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
        // @NOTE: Only call this after any related `SendStream` has been flushed with `finish()`.
        // Calling `flush()` is the responsibility of the sync engine / actor.
        connection.close(0u8.into(), b"close from disconnect");

        // @TODO: Handle the case where the connection is not explicitly closed, but has closed due
        // to error.
        // We could spawn a task where we listen for `closed()` event for each active connection.

        let peer = endpoint::get_remote_node_id(&connection)?;
        self.active_connections.remove(&peer);

        Ok(())
    }

    /// Listen for closure of the connection; this may occur due to an error or because of an
    /// action taken by the remote peer.
    async fn listen_for_disconnection(&self, connection: Connection) -> Result<()> {
        tokio::task::spawn(async move {
            let reason = connection.closed().await;
            debug!("sync connection closed: {reason}");

            // @TODO: Send `Disconnect` event on local message bus.
        })
        .await?;

        Ok(())
    }

    /// Log sync as completed for the given peer.
    fn complete_sync(&mut self, peer: NodeId) {
        // Ignore the returned `bool`.
        let _ = self.completed_sync_sessions.insert(peer);
    }

    /// Query the sync state of the given peer.
    fn sync_completed(&self, peer: &NodeId) -> bool {
        self.completed_sync_sessions.contains(peer)
    }

    /// Query the connection state of the given peer.
    fn currently_connected(&self, peer: &NodeId) -> bool {
        self.active_connections.contains(peer)
    }

    /// Handle an inbound connection attempt for the SYNC_CONNECTION ALPN.
    ///
    /// The connection will be dropped if one has already been established and is currently active.
    ///
    /// The sync actor will be invoked with the connection if a successful sync session has not
    /// previously been completed with the connecting peer.
    pub async fn handle_connection(&mut self, connection: Connection) -> Result<()> {
        let peer = endpoint::get_remote_node_id(&connection)?;

        if self.currently_connected(&peer) {
            self.disconnect(connection)?
        } else {
            let remote_addr = connection.remote_address();
            let connection_id = connection.stable_id() as u64;
            let _span = debug_span!("sync connection", connection_id, %remote_addr);

            // @TODO: Consider using a `Connected` event to avoid issues around mutability of
            // `self`.

            if !self.sync_completed(&peer) {
                self.active_connections.insert(peer);

                // @TODO: Spawn `closed()` listener.

                // @TODO: Send the connection to the sync actor.
                //
                // self.engine_actor_tx
                //    .send(ToEngineActor::SyncAccept { peer, connection })
                //    .await?;
            }
        }

        Ok(())
    }

    fn add_peer(&mut self, peer: NodeId, topic: TopicId) {
        self.address_book.insert(peer, topic);
    }

    fn remove_peer(&mut self, peer: &NodeId) {
        self.address_book.remove(peer);
    }
}

impl ProtocolHandler for ConnectionManager {
    fn accept(self: Arc<Self>, connecting: Connecting) -> BoxedFuture<Result<()>> {
        Box::pin(async move { self.handle_connection(connecting.await?).await })
    }
}
