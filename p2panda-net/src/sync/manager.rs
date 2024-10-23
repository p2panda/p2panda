// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use anyhow::{Context, Error, Result};
use iroh_gossip::proto::TopicId;
use iroh_net::{Endpoint, NodeId};
use p2panda_sync::SyncProtocol;
use thiserror::Error;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::time::{self, Duration};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, warn};

use crate::engine::ToEngineActor;
use crate::sync::{self, SYNC_CONNECTION_ALPN};

// A duration in milliseconds.
//
// This value will be used to determine the send timeout if the sync queue is full at the time
// an attempt is scheduled or rescheduled.
const SYNC_QUEUE_SEND_TIMEOUT: Duration = Duration::from_millis(100);
const SYNC_ATTEMPT_SCHEDULE_INTERVAL: Duration = Duration::from_millis(500);
const MAX_CONCURRENT_SYNC_SESSIONS: usize = 128;
const MAX_RETRY_ATTEMPTS: u8 = 5;

/// A newly discovered peer and topic combination to be sent to the sync manager.
#[derive(Debug)]
pub struct ToSyncManager {
    peer: NodeId,
    topic: TopicId,
}

impl ToSyncManager {
    pub(crate) fn new(peer: NodeId, topic: TopicId) -> Self {
        Self { peer, topic }
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
    pending_sync_sessions: VecDeque<(NodeId, TopicId)>,
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
        let (sync_queue_tx, sync_queue_rx) = mpsc::channel(MAX_CONCURRENT_SYNC_SESSIONS);
        let (sync_manager_tx, sync_manager_rx) = mpsc::channel(256);

        let sync_manager = Self {
            pending_sync_sessions: VecDeque::new(),
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
                .send_timeout(sync_attempt, SYNC_QUEUE_SEND_TIMEOUT)
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

    /// Pull the next peer and topic combination from the set of pending sessions and schedule a
    /// sync connection attempt.
    async fn schedule_next_attempt(&mut self) -> Result<()> {
        if let Some(peer_topic) = self.pending_sync_sessions.pop_back() {
            let sync_attempt = SyncAttempt::new(peer_topic.0, peer_topic.1);
            self.schedule_attempt(sync_attempt).await?;
        }

        Ok(())
    }

    /// The sync connection event loop.
    ///
    /// Listens and responds to three kinds of events
    /// - A shutdown signal from the engine
    /// - A sync attempt pulled from the queue, resulting in a call to `connect_and_sync()`
    /// - A new peer and topic combination received from the engine
    pub async fn run(mut self, token: CancellationToken) -> Result<()> {
        let mut sync_attempt_schedule_interval = time::interval(SYNC_ATTEMPT_SCHEDULE_INTERVAL);

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
                       Ok(()) => {
                           self.complete_successful_sync(sync_attempt);
                           self.schedule_next_attempt().await?
                       }
                       Err(err) => self.complete_failed_sync(sync_attempt, err).await?,
                   }
                },
                msg = self.inbox.recv() => {
                    let msg = msg.context("sync manager inbox closed")?;
                    if let Err(err) = self.update_peer_topics(msg.peer, msg.topic).await {
                        warn!("failed to update peer topics: {}", err)
                    }
                }
                _ = sync_attempt_schedule_interval.tick() => {
                    if let Some((peer, topic)) = self.pending_sync_sessions.pop_front() {
                        let sync_attempt = SyncAttempt::new(peer, topic);
                        self.schedule_attempt(sync_attempt).await?
                    }
                }
            }
        }

        Ok(())
    }

    fn is_active(&self, peer: &NodeId, topic: &TopicId) -> bool {
        if let Some(peers) = self.active_sync_sessions.get(topic) {
            peers.contains(peer)
        } else {
            false
        }
    }

    fn is_complete(&self, peer: &NodeId, topic: &TopicId) -> bool {
        if let Some(peers) = self.completed_sync_sessions.get(topic) {
            peers.contains(peer)
        } else {
            false
        }
    }

    fn is_pending(&self, peer: NodeId, topic: TopicId) -> bool {
        self.pending_sync_sessions.contains(&(peer, topic))
    }

    /// Store a newly discovered peer and topic combination.
    async fn update_peer_topics(&mut self, peer: NodeId, topic: TopicId) -> Result<()> {
        debug!("updating peer topics in connection manager");

        // Insert the peer-topic combination into our set of known peers.
        if let Some(known_topics) = self.known_peer_topics.get_mut(&peer) {
            known_topics.insert(topic);
        } else {
            let mut topics = HashSet::new();
            topics.insert(topic);
            self.known_peer_topics.insert(peer, topics);
        }

        // Insert the peer-topic combination into the set of pending sync sessions if it is not
        // already pending, active or complete.
        if !self.is_active(&peer, &topic)
            && !self.is_complete(&peer, &topic)
            && !self.is_pending(peer, topic)
        {
            self.pending_sync_sessions.push_back((peer, topic))
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
