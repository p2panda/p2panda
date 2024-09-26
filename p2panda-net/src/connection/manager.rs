// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::Result;
use iroh_gossip::proto::TopicId;
use iroh_net::{Endpoint, NodeId};
use p2panda_sync::traits::SyncProtocol;
use p2panda_sync::SyncError;
use tokio::sync::mpsc::Sender;
use tracing::{debug, warn};

use crate::connection::sync;
use crate::connection::SYNC_CONNECTION_ALPN;
use crate::engine::ToEngineActor;

#[derive(Debug)]
pub struct ConnectionManager {
    known_peer_topics: HashMap<NodeId, HashSet<TopicId>>,
    active_sync_sessions: HashMap<TopicId, HashSet<NodeId>>,
    completed_sync_sessions: HashMap<TopicId, HashSet<NodeId>>,
    engine_actor_tx: Sender<ToEngineActor>,
    endpoint: Endpoint,
    sync_protocol: Option<Arc<dyn for<'a> SyncProtocol<'a> + 'static>>,
}

impl ConnectionManager {
    pub fn new(
        endpoint: Endpoint,
        engine_actor_tx: Sender<ToEngineActor>,
        sync_protocol: Option<Arc<dyn for<'a> SyncProtocol<'a> + 'static>>,
    ) -> Self {
        Self {
            known_peer_topics: HashMap::new(),
            active_sync_sessions: HashMap::new(),
            completed_sync_sessions: HashMap::new(),
            engine_actor_tx,
            endpoint,
            sync_protocol,
        }
    }

    /// Respond to newly discovered peer topics by initiating a new connection if one is not
    /// currently underway and a successful sync session has not already been completed.
    pub async fn update_peer_topics(&mut self, peer: NodeId, topics: Vec<TopicId>) -> Result<()> {
        debug!("updating peer topics in connection manager");

        let known_topics = self.known_peer_topics.get(&peer);
        let mut new_topics = Vec::new();
        if let Some(known_topics) = known_topics {
            for topic in topics {
                if !known_topics.contains(&topic) {
                    new_topics.push(topic)
                }
            }
        } else {
            new_topics = topics
        }

        for topic in new_topics {
            // Peers with whom we have active outbound sync sessions for this topic.
            let active_peers = self.active_sync_sessions.entry(topic).or_default();

            // Have we already completed a successful sync session with this peer?
            let sync_complete = self
                .completed_sync_sessions
                .entry(topic)
                .or_default()
                .contains(&peer);

            // Attempt connection in order to initiate a sync session.
            if !active_peers.contains(&peer) && !sync_complete {
                active_peers.insert(peer);
                // @TODO: We might want to disentangle this call from the `update_peer_topics`
                // method so that it returns immediately and the `connect` method has its own life.
                if let Err(err) = self.connect(peer, topic).await {
                    warn!("outbound connection attempt failed: {}", err)
                }
            }
        }

        Ok(())
    }

    /// Attempt to connect with the given peer.
    ///
    /// A `None` value will be returned if a connection has already been established and is
    /// currently active.
    pub async fn connect(&mut self, peer: NodeId, topic: TopicId) -> Result<()> {
        debug!("attempting peer connection for sync");

        let connection = self
            .endpoint
            .connect_by_node_id(peer, SYNC_CONNECTION_ALPN)
            .await?;

        // Create a bidirectional stream on the connection.
        let (mut send, mut recv) = connection
            .open_bi()
            .await
            .map_err(|e| SyncError::Protocol(e.to_string()))?;

        let sync_protocol = self
            .sync_protocol
            .clone()
            .expect("sync protocol has been defined");
        let engine_actor_tx = self.engine_actor_tx.clone();

        // Run a sync session as the initiator.
        let result = sync::initiate_sync(
            &mut send,
            &mut recv,
            peer,
            topic,
            sync_protocol,
            engine_actor_tx,
        )
        .await;

        // Clean-up the streams.
        send.finish()?;
        send.stopped().await?;
        recv.read_to_end(0).await?;

        if result.is_ok() {
            debug!("sync success: initiate");
            self.complete_successful_sync(peer, topic)
        } else {
            debug!("sync failure. initiate");
            self.complete_failed_sync(peer, topic)
        }

        Ok(())
    }

    /// Remove the given topic from the set of active sync sessions for the given peer.
    pub fn complete_failed_sync(&mut self, peer: NodeId, topic: TopicId) {
        // @TODO: Add peer to the retry queue.

        self.active_sync_sessions
            .get_mut(&topic)
            .expect("active outbound sync session exists")
            .remove(&peer);
    }

    /// Remove the given topic from the set of active sync sessions for the given peer and add them
    /// to the set of completed sync sessions.
    pub fn complete_successful_sync(&mut self, peer: NodeId, topic: TopicId) {
        self.active_sync_sessions
            .get_mut(&topic)
            .expect("active outbound sync session exists")
            .remove(&peer);

        self.completed_sync_sessions
            .entry(topic)
            .or_default()
            .insert(peer);
    }
}
