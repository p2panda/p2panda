// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use iroh_gossip::proto::TopicId;
use iroh_net::endpoint::Connection;
use iroh_net::{Endpoint, NodeId};
use p2panda_sync::SyncError;
use tokio::sync::mpsc::Sender;

use crate::connection::{ToConnectionActor, SYNC_CONNECTION_ALPN};

#[derive(Debug)]
pub struct ConnectionManager {
    peers: HashMap<NodeId, HashSet<TopicId>>,
    outbound_sync_sessions: HashMap<TopicId, HashSet<NodeId>>,
    completed_sync_sessions: HashMap<TopicId, HashSet<NodeId>>,
    connection_actor_tx: Sender<ToConnectionActor>,
    endpoint: Endpoint,
}

impl ConnectionManager {
    pub fn new(endpoint: Endpoint, connection_actor_tx: Sender<ToConnectionActor>) -> Self {
        Self {
            peers: HashMap::new(),
            outbound_sync_sessions: HashMap::new(),
            completed_sync_sessions: HashMap::new(),
            connection_actor_tx,
            endpoint,
        }
    }

    /// Respond to newly discovered peer topics by initiating a new connection if one is not
    /// currently underway and a successful sync session has not already been completed.
    pub async fn update_peer_topics(&mut self, peer: NodeId, topics: Vec<TopicId>) -> Result<()> {
        let new_topics = HashSet::from_iter(topics.into_iter());
        let old_topics = self.peers.entry(peer).or_default();
        let difference = new_topics.difference(old_topics);

        for topic in difference {
            // Peers with whom we have active outbound sync sessions for this topic.
            let outbound_peers = self
                .outbound_sync_sessions
                .entry(*topic)
                .or_insert(HashSet::new());

            // Have we already completed a successful sync session with this peer?
            let sync_complete = self
                .completed_sync_sessions
                .entry(*topic)
                .or_default()
                .contains(&peer);

            // Attempt connection in order to initiate a sync session.
            if !outbound_peers.contains(&peer) && !sync_complete {
                outbound_peers.insert(peer);
                self.connection_actor_tx
                    .send(ToConnectionActor::Connect {
                        peer,
                        topic: *topic,
                    })
                    .await?;
            }
        }

        Ok(())
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

    /// Update sync session status for the given peer.
    pub fn complete_failed_sync(&mut self, peer: NodeId, topic: TopicId, _err: SyncError) {
        // @TODO: Add peer to the retry queue.

        self.outbound_sync_sessions
            .get_mut(&topic)
            .expect("outbound sync session exists")
            .remove(&peer);
    }

    /// Update sync session status for the given peer.
    pub fn complete_successful_sync(&mut self, peer: NodeId, topic: TopicId) {
        self.outbound_sync_sessions
            .get_mut(&topic)
            .expect("outbound sync session exists")
            .remove(&peer);
        self.completed_sync_sessions
            .entry(topic)
            .or_default()
            .insert(peer);
    }
}
