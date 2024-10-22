// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::{Context, Error, Result};
use iroh_gossip::proto::TopicId;
use iroh_net::{Endpoint, NodeId};
use p2panda_sync::SyncProtocol;
use thiserror::Error;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, warn};

use crate::engine::ToEngineActor;
use crate::sync::{self, SYNC_CONNECTION_ALPN};

const MAX_RETRY_ATTEMPTS: u8 = 5;

#[derive(Debug)]
pub struct ToSyncManager {
    peer: NodeId,
    topics: Vec<TopicId>,
}

impl ToSyncManager {
    pub fn new(peer: NodeId, topics: Vec<TopicId>) -> Self {
        Self { peer, topics }
    }
}

#[derive(Debug)]
struct SyncAttempt {
    peer: NodeId,
    topic: TopicId,
    attempts: u8,
}

impl SyncAttempt {
    fn new(peer: NodeId, topic: TopicId) -> Self {
        Self {
            peer,
            topic,
            attempts: 0,
        }
    }
}

#[derive(Debug, Error)]
enum SyncAttemptError {
    #[error("connection error")]
    Connection,
    #[error("sync error")]
    Sync,
}

#[derive(Debug)]
pub struct SyncManager {
    active_sync_sessions: HashMap<TopicId, HashSet<NodeId>>,
    completed_sync_sessions: HashMap<TopicId, HashSet<NodeId>>,
    endpoint: Endpoint,
    engine_actor_tx: Sender<ToEngineActor>,
    inbox: Receiver<ToSyncManager>,
    known_peer_topics: HashMap<NodeId, HashSet<TopicId>>,
    max_retry_attempts: u8,
    sync_protocol: Arc<dyn for<'a> SyncProtocol<'a> + 'static>,
    sync_queue_tx: Sender<SyncAttempt>,
    sync_queue_rx: Receiver<SyncAttempt>,
}

impl SyncManager {
    pub fn new(
        endpoint: Endpoint,
        engine_actor_tx: Sender<ToEngineActor>,
        inbox: Option<Receiver<ToSyncManager>>,
        sync_protocol: Arc<dyn for<'a> SyncProtocol<'a> + 'static>,
    ) -> Self {
        let inbox = inbox.expect("channel receiver provided to sync manager");
        let (sync_queue_tx, sync_queue_rx) = mpsc::channel(128);

        Self {
            active_sync_sessions: HashMap::new(),
            completed_sync_sessions: HashMap::new(),
            endpoint,
            engine_actor_tx,
            inbox,
            known_peer_topics: HashMap::new(),
            max_retry_attempts: MAX_RETRY_ATTEMPTS,
            sync_protocol,
            sync_queue_tx,
            sync_queue_rx,
        }
    }

    async fn schedule_attempt(&mut self, peer: NodeId, topic: TopicId) -> Result<()> {
        let sync_attempt = SyncAttempt::new(peer, topic);
        self.sync_queue_tx.send(sync_attempt).await?;

        Ok(())
    }

    async fn reschedule_attempt(&mut self, mut sync_attempt: SyncAttempt) -> Result<()> {
        sync_attempt.attempts += 1;
        self.sync_queue_tx.send(sync_attempt).await?;

        Ok(())
    }

    /// Connection + sync loop.
    pub async fn run(mut self, token: CancellationToken) -> Result<()> {
        loop {
            tokio::select! {
                biased;

                _ = token.cancelled() => {
                    debug!("sync manager received shutdown signal from engine");
                    break;
                }
                Some(sync_attempt) = self.sync_queue_rx.recv() => {
                    match self
                       .connect_and_sync(sync_attempt.peer, sync_attempt.topic)
                       .await
                   {
                       Ok(()) => self.complete_successful_sync(sync_attempt),
                       Err(err) => self.complete_failed_sync(sync_attempt, err).await?,
                   }
                },
                msg = self.inbox.recv() => {
                    let msg = msg.context("sync manager inbox closed")?;
                    self.update_peer_topics(msg.peer, msg.topics).await?
                }
            }
        }

        Ok(())
    }

    /// Respond to newly discovered peer topics by initiating a new connection if one is not
    /// currently underway and a successful sync session has not already been completed.
    async fn update_peer_topics(&mut self, peer: NodeId, topics: Vec<TopicId>) -> Result<()> {
        debug!("updating peer topics in connection manager");

        // Create a list of (previously) unknown topics that we might like to sync over.
        let mut new_topics = Vec::new();
        let known_topics = self.known_peer_topics.get(&peer);
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

            // Schedule a sync attempt.
            if !active_peers.contains(&peer) && !sync_complete {
                self.schedule_attempt(peer, topic).await?;
            }
        }

        Ok(())
    }

    /// Attempt to connect with the given peer and initiate a sync session.
    pub async fn connect_and_sync(&mut self, peer: NodeId, topic: TopicId) -> Result<()> {
        debug!("attempting peer connection for sync");

        let connection = self
            .endpoint
            .connect_by_node_id(peer, SYNC_CONNECTION_ALPN)
            .await
            .map_err(|_| SyncAttemptError::Connection)?;

        // Create a bidirectional stream on the connection.
        let (mut send, mut recv) = connection
            .open_bi()
            .await
            .map_err(|_| SyncAttemptError::Connection)?;

        let sync_protocol = self.sync_protocol.clone();
        let engine_actor_tx = self.engine_actor_tx.clone();

        // Run a sync session as the initiator.
        sync::initiate_sync(
            &mut send,
            &mut recv,
            peer,
            topic,
            sync_protocol,
            engine_actor_tx,
        )
        .await
        .map_err(|_| SyncAttemptError::Sync)?;

        // Clean-up the streams.
        send.finish()?;
        send.stopped().await?;
        recv.read_to_end(0).await?;

        Ok(())
    }

    /// Remove the given topic from the set of active sync sessions for the given peer.
    async fn complete_failed_sync(&mut self, sync_attempt: SyncAttempt, err: Error) -> Result<()> {
        self.active_sync_sessions
            .get_mut(&sync_attempt.topic)
            .expect("active outbound sync session exists")
            .remove(&sync_attempt.peer);

        match err.downcast_ref() {
            Some(SyncAttemptError::Connection) => {
                warn!("sync attempt failed due to connection error");
                if sync_attempt.attempts <= self.max_retry_attempts {
                    self.reschedule_attempt(sync_attempt).await?;
                }
            }
            Some(SyncAttemptError::Sync) => {
                error!("sync attempt failed due to protocol error");
            }
            _ => (),
        }

        Ok(())
    }

    /// Remove the given topic from the set of active sync sessions for the given peer and add them
    /// to the set of completed sync sessions.
    fn complete_successful_sync(&mut self, sync_attempt: SyncAttempt) {
        self.active_sync_sessions
            .get_mut(&sync_attempt.topic)
            .expect("active outbound sync session exists")
            .remove(&sync_attempt.peer);

        self.completed_sync_sessions
            .entry(sync_attempt.topic)
            .or_default()
            .insert(sync_attempt.peer);
    }
}
