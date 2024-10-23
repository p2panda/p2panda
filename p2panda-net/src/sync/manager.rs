// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::{Context, Error, Result};
use iroh_gossip::proto::TopicId;
use iroh_net::{Endpoint, NodeId};
use p2panda_sync::SyncProtocol;
use thiserror::Error;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, warn};

use crate::engine::ToEngineActor;
use crate::sync::{self, SYNC_CONNECTION_ALPN};

// A duration in milliseconds.
//
// This value will be used to determine the send timeout if the sync queue is full at the time
// an attempt is scheduled or rescheduled.
const SYNC_QUEUE_SEND_TIMEOUT: u64 = 100;
const MAX_RETRY_ATTEMPTS: u8 = 5;

/// A newly discovered peer and topic combination to be sent to the sync manager.
#[derive(Debug)]
pub struct ToSyncManager {
    peer: NodeId,
    topics: Vec<TopicId>,
}

impl ToSyncManager {
    pub(crate) fn new(peer: NodeId, topics: Vec<TopicId>) -> Self {
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

/// An API for scheduling outbound connections and sync attempts.
#[derive(Debug)]
pub(crate) struct SyncManager {
    active_sync_sessions: HashMap<TopicId, HashSet<NodeId>>,
    completed_sync_sessions: HashMap<TopicId, HashSet<NodeId>>,
    endpoint: Endpoint,
    engine_actor_tx: Sender<ToEngineActor>,
    inbox: Receiver<ToSyncManager>,
    known_peer_topics: HashMap<NodeId, HashSet<TopicId>>,
    sync_protocol: Arc<dyn for<'a> SyncProtocol<'a> + 'static>,
    sync_queue_tx: Sender<SyncAttempt>,
    sync_queue_rx: Receiver<SyncAttempt>,
}

impl SyncManager {
    /// Create a new instance of the `SyncManager` and return it along with a channel sender.
    pub(crate) fn new(
        endpoint: Endpoint,
        engine_actor_tx: Sender<ToEngineActor>,
        sync_protocol: Arc<dyn for<'a> SyncProtocol<'a> + 'static>,
    ) -> (Self, Sender<ToSyncManager>) {
        let (sync_queue_tx, sync_queue_rx) = mpsc::channel(128);
        let (sync_manager_tx, sync_manager_rx) = mpsc::channel(256);

        let sync_manager = Self {
            active_sync_sessions: HashMap::new(),
            completed_sync_sessions: HashMap::new(),
            endpoint,
            engine_actor_tx,
            inbox: sync_manager_rx,
            known_peer_topics: HashMap::new(),
            sync_protocol,
            sync_queue_tx,
            sync_queue_rx,
        };

        (sync_manager, sync_manager_tx)
    }

    /// Add a peer and topic combination to the sync connection queue.
    async fn schedule_attempt(&mut self, sync_attempt: SyncAttempt) -> Result<()> {
        // Only send if the queue is not full; this prevents the possibility of blocking on send.
        if self.sync_queue_tx.capacity() < self.sync_queue_tx.max_capacity() {
            self.sync_queue_tx.send(sync_attempt).await?;
        } else {
            self.sync_queue_tx
                .send_timeout(sync_attempt, Duration::from_millis(SYNC_QUEUE_SEND_TIMEOUT))
                .await?;
        }

        Ok(())
    }

    /// Add a peer and topic combination to the sync connection queue, incrementing the number of
    /// previous attempts.
    async fn reschedule_attempt(&mut self, mut sync_attempt: SyncAttempt) -> Result<()> {
        sync_attempt.attempts += 1;
        self.schedule_attempt(sync_attempt).await?;

        Ok(())
    }

    /// The sync connection event loop.
    ///
    /// Listens and responds to three kinds of events
    /// - A shutdown signal from the engine
    /// - A sync attempt pulled from the queue, resulting in a call to `connect_and_sync()`
    /// - A new peer and topic combination received from the engine
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
                    if let Err(err) = self.update_peer_topics(msg.peer, msg.topics).await {
                        warn!("failed to update peer topics: {}", err)
                    }
                }
            }
        }

        Ok(())
    }

    /// Store a newly discovered peer and topic combination and schedule a sync connection
    /// attempt if one is not currently underway and a successful sync session has not
    /// already been completed.
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
                let sync_attempt = SyncAttempt::new(peer, topic);
                self.schedule_attempt(sync_attempt).await?;
            }
        }

        Ok(())
    }

    /// Attempt to connect with the given peer and initiate a sync session.
    async fn connect_and_sync(&mut self, peer: NodeId, topic: TopicId) -> Result<()> {
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

    /// Remove the given topic from the set of active sync sessions for the given peer. Reschedule
    /// a sync attempt if the failure was caused by a connection error. Drop the attempt if the
    /// failure occurred due to a sync protocol error.
    async fn complete_failed_sync(&mut self, sync_attempt: SyncAttempt, err: Error) -> Result<()> {
        self.active_sync_sessions
            .get_mut(&sync_attempt.topic)
            .expect("active outbound sync session exists")
            .remove(&sync_attempt.peer);

        match err.downcast_ref() {
            Some(SyncAttemptError::Connection) => {
                warn!("sync attempt failed due to connection error");
                if sync_attempt.attempts <= MAX_RETRY_ATTEMPTS {
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
