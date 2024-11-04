// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use anyhow::{Context, Error, Result};
use iroh_net::{Endpoint, NodeId};
use p2panda_sync::{SyncProtocol, Topic};
use thiserror::Error;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, warn};

use crate::engine::ToEngineActor;
use crate::sync::{self, SYNC_CONNECTION_ALPN};
use crate::TopicId;

/// This value will be used to determine the send timeout if the sync queue is full at the time an
/// attempt is scheduled or rescheduled.
const SYNC_QUEUE_SEND_TIMEOUT: Duration = Duration::from_millis(100);

const MAX_CONCURRENT_SYNC_SESSIONS: usize = 128;

const MAX_RETRY_ATTEMPTS: u8 = 5;

/// A newly discovered peer and topic combination to be sent to the sync manager.
#[derive(Debug)]
pub struct ToSyncActor<T> {
    peer: NodeId,
    topic: T,
}

impl<T> ToSyncActor<T> {
    pub(crate) fn new(peer: NodeId, topic: T) -> Self {
        Self { peer, topic }
    }
}

#[derive(Debug)]
struct SyncAttempt<T> {
    peer: NodeId,
    topic: T,
    attempts: u8,
}

impl<T> SyncAttempt<T> {
    fn new(peer: NodeId, topic: T) -> Self {
        Self {
            peer,
            topic,
            attempts: 0,
        }
    }
}

#[derive(Debug, Error)]
enum SyncAttemptError {
    /// Error occurred while attempting to connect to a peer or while attempting to open a
    /// bidirectional stream.
    #[error("sync attempt failed due to connection or stream error")]
    Connection,

    /// Error occurred while initiating a sync session.
    #[error("sync attempt failed due to sync protocol error")]
    Sync,
}

/// An API for scheduling outbound connections and sync attempts.
#[derive(Debug)]
pub(crate) struct SyncActor<T> {
    pending_sync_sessions: VecDeque<(NodeId, T)>,
    active_sync_sessions: HashMap<T, HashSet<NodeId>>,
    completed_sync_sessions: HashMap<T, HashSet<NodeId>>,
    endpoint: Endpoint,
    engine_actor_tx: Sender<ToEngineActor<T>>,
    inbox: Receiver<ToSyncActor<T>>,
    sync_protocol: Arc<dyn for<'a> SyncProtocol<'a, T> + 'static>,
    sync_queue_tx: Sender<SyncAttempt<T>>,
    sync_queue_rx: Receiver<SyncAttempt<T>>,
}

impl<T> SyncActor<T>
where
    T: Topic + TopicId + 'static,
{
    /// Create a new instance of the `SyncActor` and return it along with a channel sender.
    pub(crate) fn new(
        endpoint: Endpoint,
        engine_actor_tx: Sender<ToEngineActor<T>>,
        sync_protocol: Arc<dyn for<'a> SyncProtocol<'a, T> + 'static>,
    ) -> (Self, Sender<ToSyncActor<T>>) {
        let (sync_queue_tx, sync_queue_rx) = mpsc::channel(MAX_CONCURRENT_SYNC_SESSIONS);
        let (sync_manager_tx, sync_manager_rx) = mpsc::channel(256);

        let sync_manager = Self {
            pending_sync_sessions: VecDeque::new(),
            active_sync_sessions: HashMap::new(),
            completed_sync_sessions: HashMap::new(),
            endpoint,
            engine_actor_tx,
            inbox: sync_manager_rx,
            sync_protocol,
            sync_queue_tx,
            sync_queue_rx,
        };

        (sync_manager, sync_manager_tx)
    }

    /// Add a peer and topic combination to the sync connection queue.
    async fn schedule_attempt(&mut self, sync_attempt: SyncAttempt<T>) -> Result<()> {
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
    async fn reschedule_attempt(&mut self, mut sync_attempt: SyncAttempt<T>) -> Result<()> {
        sync_attempt.attempts += 1;
        self.schedule_attempt(sync_attempt).await?;

        Ok(())
    }

    /// Pull the next peer and topic combination from the set of pending sessions and schedule a
    /// sync connection attempt.
    async fn schedule_next_attempt(&mut self) -> Result<()> {
        if let Some(peer_topic) = self.pending_sync_sessions.pop_front() {
            let sync_attempt = SyncAttempt::new(peer_topic.0, peer_topic.1.clone());

            // Scheduling the attempt will fail if the sync queue is full. In that case, we return
            // the peer and topic combination to the buffer of pending sync sessions.
            if self.schedule_attempt(sync_attempt).await.is_err() {
                self.pending_sync_sessions.push_back(peer_topic)
            }
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
        loop {
            tokio::select! {
                biased;

                _ = token.cancelled() => {
                    debug!("sync manager received shutdown signal from engine");
                    break;
                }
                Some(sync_attempt) = self.sync_queue_rx.recv() => {
                    match self
                       .connect_and_sync(sync_attempt.peer, sync_attempt.topic.clone())
                       .await
                   {
                       Ok(()) => self.complete_successful_sync(sync_attempt).await?,
                       Err(err) => self.complete_failed_sync(sync_attempt, err).await?,
                   }
                },
                msg = self.inbox.recv() => {
                    let msg = msg.context("sync manager inbox closed")?;
                    let peer = msg.peer;
                    let topic = msg.topic;

                    // Keep track of all concrete topics we will be running sync sessions over.
                    self.queue_sync_session(peer, topic).await;
                    self.schedule_next_attempt().await?
                }
            }
        }

        Ok(())
    }

    /// Do we have an active sync session underway for the given peer topic combination?
    fn is_active(&self, peer: &NodeId, topic: &T) -> bool {
        if let Some(peers) = self.active_sync_sessions.get(topic) {
            peers.contains(peer)
        } else {
            false
        }
    }

    /// Do we have a complete sync session for the given peer topic combination?
    fn is_complete(&self, peer: &NodeId, topic: &T) -> bool {
        if let Some(peers) = self.completed_sync_sessions.get(topic) {
            peers.contains(peer)
        } else {
            false
        }
    }

    /// Do we have a pending sync session for the given peer topic combination?
    fn is_pending(&self, peer: NodeId, topic: T) -> bool {
        self.pending_sync_sessions.contains(&(peer, topic))
    }

    /// Conditionally insert the peer-topic combination into the set of pending sync sessions.
    async fn queue_sync_session(&mut self, peer: NodeId, topic: T) {
        if !self.is_active(&peer, &topic)
            && !self.is_complete(&peer, &topic)
            && !self.is_pending(peer, topic.clone())
        {
            self.pending_sync_sessions.push_back((peer, topic))
        }
    }

    /// Attempt to connect with the given peer and initiate a sync session.
    async fn connect_and_sync(&mut self, peer: NodeId, topic: T) -> Result<()> {
        debug!("attempting peer connection for sync");

        let connection = self
            .endpoint
            .connect_by_node_id(peer, SYNC_CONNECTION_ALPN)
            .await
            .map_err(|_| SyncAttemptError::Connection)?;

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
            topic.clone(),
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
    /// a sync attempt if the failure was caused by a connection error. Otherwise, drop the attempt
    /// and schedule the next pending attempt.
    async fn complete_failed_sync(
        &mut self,
        sync_attempt: SyncAttempt<T>,
        err: Error,
    ) -> Result<()> {
        self.active_sync_sessions
            .get_mut(&sync_attempt.topic)
            .expect("active outbound sync session exists")
            .remove(&sync_attempt.peer);

        if let Some(SyncAttemptError::Connection) = err.downcast_ref() {
            warn!("sync attempt failed due to connection error");
            if sync_attempt.attempts <= MAX_RETRY_ATTEMPTS {
                self.reschedule_attempt(sync_attempt).await?;
                return Ok(());
            }
        }

        self.schedule_next_attempt().await?;

        Ok(())
    }

    /// Remove the given topic from the set of active sync sessions for the given peer and add them
    /// to the set of completed sync sessions. Then schedule the next pending attempt.
    async fn complete_successful_sync(&mut self, sync_attempt: SyncAttempt<T>) -> Result<()> {
        self.active_sync_sessions
            .get_mut(&sync_attempt.topic)
            .expect("active outbound sync session exists")
            .remove(&sync_attempt.peer);

        self.completed_sync_sessions
            .entry(sync_attempt.topic)
            .or_default()
            .insert(sync_attempt.peer);

        self.schedule_next_attempt().await?;

        Ok(())
    }
}
