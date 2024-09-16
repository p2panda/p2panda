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
//   - I think...
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
use tracing::debug_span;

use crate::protocols::ProtocolHandler;

// @TODO: Look at `PeerMap` in `src/engine/engine.rs`
// That contains some address book functionality.
// Be sure we're not duplicating efforts.

pub const SYNC_CONNECTION_ALPN: &[u8] = b"/p2panda-net-sync/0";

#[derive(Debug)]
struct ConnectionManager {
    active_connections: HashSet<NodeId>,
    address_book: HashMap<NodeId, TopicId>,
    endpoint: Endpoint,
}

impl ConnectionManager {
    pub fn new(endpoint: Endpoint) -> Self {
        Self {
            active_connections: HashSet::new(),
            address_book: HashMap::new(),
            endpoint,
        }
    }

    pub async fn connect(&mut self, peer: NodeId) -> Result<Connection> {
        let connection = self
            .endpoint
            .connect_by_node_id(peer, SYNC_CONNECTION_ALPN)
            .await?;

        self.active_connections.insert(peer);

        Ok(connection)
    }

    pub fn disconnect(&mut self, connection: Connection) -> Result<()> {
        // @NOTE: Only call this after any related `SendStream` has been flushed with `finish()`.
        connection.close(0u8.into(), b"close from disconnect");

        // @TODO: Handle the case where the connection is not explicitly closed, but has closed due
        // to error.
        // We could spawn a task where we listen for `closed()` event for each active connection.

        let peer = endpoint::get_remote_node_id(&connection)?;
        self.active_connections.remove(&peer);

        Ok(())
    }

    pub async fn handle_connection(&self, connection: Connection) -> Result<()> {
        let peer = endpoint::get_remote_node_id(&connection)?;
        let remote_addr = connection.remote_address();
        let connection_id = connection.stable_id() as u64;
        let _span = debug_span!("connection", connection_id, %remote_addr);

        // @TODO: Send the connection to the sync actor.
        //
        // self.engine_actor_tx
        //    .send(ToEngineActor::SyncAccept { peer, connection })
        //    .await?;

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
