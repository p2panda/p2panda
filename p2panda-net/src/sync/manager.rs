// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{HashMap, HashSet, VecDeque};

use anyhow::{Context, Error, Result};
use iroh_net::{Endpoint, NodeId};
use p2panda_sync::{SyncError, Topic};
use thiserror::Error;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::time::{interval, Duration, Instant};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, trace, warn};

use crate::engine::ToEngineActor;
use crate::sync::{self, SYNC_CONNECTION_ALPN};
use crate::TopicId;

use super::SyncConfiguration;

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
    completed: Option<Instant>,
}

impl<T> SyncAttempt<T> {
    fn new(peer: NodeId, topic: T) -> Self {
        Self {
            peer,
            topic,
            attempts: 0,
            completed: None,
        }
    }
}

#[derive(Debug, Error)]
enum SyncAttemptError {
    /// Error occurred while attempting to connect to a peer or while attempting to open a
    /// bidirectional stream.
    #[error("sync attempt failed due to connection or stream error")]
    Connection,

    /// Error occurred while initiating or accepting a sync session.
    #[error(transparent)]
    Sync(#[from] SyncError),
}

/// An API for scheduling outbound connections and sync attempts.
#[derive(Debug)]
pub(crate) struct SyncActor<T> {
    config: SyncConfiguration<T>,
    pending_sync_sessions: HashMap<T, HashSet<NodeId>>,
    active_sync_sessions: HashMap<T, HashSet<NodeId>>,
    completed_sync_sessions: HashMap<T, HashSet<NodeId>>,
    endpoint: Endpoint,
    engine_actor_tx: Sender<ToEngineActor<T>>,
    inbox: Receiver<ToSyncActor<T>>,
    resync_queue: VecDeque<SyncAttempt<T>>,
    sync_queue_tx: Sender<SyncAttempt<T>>,
    sync_queue_rx: Receiver<SyncAttempt<T>>,
}

impl<T> SyncActor<T>
where
    T: Topic + TopicId + 'static,
{
    /// Create a new instance of the `SyncActor` and return it along with a channel sender.
    pub(crate) fn new(
        config: SyncConfiguration<T>,
        endpoint: Endpoint,
        engine_actor_tx: Sender<ToEngineActor<T>>,
    ) -> (Self, Sender<ToSyncActor<T>>) {
        let (sync_queue_tx, sync_queue_rx) = mpsc::channel(config.max_concurrent_sync_sessions);
        let (sync_manager_tx, sync_manager_rx) = mpsc::channel(256);

        let sync_manager = Self {
            config,
            pending_sync_sessions: HashMap::new(),
            active_sync_sessions: HashMap::new(),
            completed_sync_sessions: HashMap::new(),
            endpoint,
            engine_actor_tx,
            inbox: sync_manager_rx,
            resync_queue: VecDeque::new(),
            sync_queue_tx,
            sync_queue_rx,
        };

        (sync_manager, sync_manager_tx)
    }

    /// Add a peer and topic combination to the sync connection queue.
    async fn schedule_attempt(
        &mut self,
        sync_attempt: SyncAttempt<T>,
        is_resync: bool,
    ) -> Result<()> {
        if self.is_pending(&sync_attempt.peer, &sync_attempt.topic)
            || self.is_active(&sync_attempt.peer, &sync_attempt.topic)
        {
            return Ok(());
        }

        if self.is_complete(&sync_attempt.peer, &sync_attempt.topic) && !is_resync {
            return Ok(());
        }

        self.pending_sync_sessions
            .entry(sync_attempt.topic.clone())
            .or_default()
            .insert(sync_attempt.peer);

        // Only send if the queue is not full; this prevents the possibility of blocking on send.
        if self.sync_queue_tx.capacity() < self.sync_queue_tx.max_capacity() {
            self.sync_queue_tx.send(sync_attempt).await?;
        } else {
            self.sync_queue_tx
                .send_timeout(sync_attempt, self.config.sync_queue_send_timeout)
                .await?;
        }

        Ok(())
    }

    /// Add a peer and topic combination to the sync connection queue, incrementing the number of
    /// previous attempts.
    async fn reschedule_attempt(&mut self, mut sync_attempt: SyncAttempt<T>) -> Result<()> {
        sync_attempt.attempts += 1;
        let is_resync = self.config.is_resync();
        self.schedule_attempt(sync_attempt, is_resync).await?;

        Ok(())
    }

    /// The sync connection event loop.
    ///
    /// Listens and responds to three kinds of events:
    ///
    /// - A shutdown signal from the engine
    /// - A sync attempt pulled from the queue, resulting in a call to `connect_and_sync()`
    /// - A new peer and topic combination received from the engine
    /// - A tick of the resync poll interval, resulting in a resync attempt if one is in the queue
    pub async fn run(mut self, token: CancellationToken) -> Result<()> {
        // Define the resync intervals based on supplied configuration parameters if resync has
        // been enabled. Otherwise create long-duration fallback values; this is mostly just
        // necessary for the resync poll interval tick.
        let (mut resync_poll_interval, resync_interval, is_resync) =
            if let Some(ref resync) = self.config.resync {
                (interval(resync.poll_interval), resync.interval, true)
            } else {
                let one_hour = Duration::from_secs(3600);
                (interval(one_hour), one_hour, false)
            };

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

                    let sync_attempt = SyncAttempt::new(peer, topic);

                    if let Err(err) = self.schedule_attempt(sync_attempt, is_resync).await {
                        // The attempt will fail if the sync queue is full, indicating that a high
                        // volume of sync sessions are underway. In that case, we drop the attempt
                        // completely. Another attempt will be scheduled when the next announcement of
                        // this peer-topic combination is received from the network-wide gossip
                        // overlay.
                        error!("failed to schedule sync attempt: {}", err)
                    }
                }
                _ = resync_poll_interval.tick() => {
                    if let Some(attempt) = self.resync_queue.pop_front() {
                        if let Some(completion) = attempt.completed {
                            if completion.elapsed() >= resync_interval {
                                trace!("schedule resync attempt {attempt:?}");
                                if let Err(err) = self.schedule_attempt(attempt, is_resync).await {
                                    error!("failed to schedule resync attempt: {}", err)
                                }
                            } else {
                                self.resync_queue.push_back(attempt)
                            }
                        }
                    }
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
    fn is_pending(&self, peer: &NodeId, topic: &T) -> bool {
        if let Some(peers) = self.pending_sync_sessions.get(topic) {
            peers.contains(peer)
        } else {
            false
        }
    }

    /// Attempt to connect with the given peer and initiate a sync session.
    async fn connect_and_sync(&mut self, peer: NodeId, topic: T) -> Result<()> {
        debug!("attempting peer connection for sync");

        self.active_sync_sessions
            .entry(topic.clone())
            .or_default()
            .insert(peer);

        if let Some(session) = self.pending_sync_sessions.get_mut(&topic) {
            session.remove(&peer);
        }

        let connection = self
            .endpoint
            .connect_by_node_id(peer, SYNC_CONNECTION_ALPN)
            .await
            .map_err(|_| SyncAttemptError::Connection)?;

        let (mut send, mut recv) = connection
            .open_bi()
            .await
            .map_err(|_| SyncAttemptError::Connection)?;

        let sync_protocol = self.config.protocol();
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
        .await?;

        // Clean-up the streams.
        send.finish()?;
        send.stopped().await?;
        recv.read_to_end(0).await?;

        Ok(())
    }

    /// Clean up after a failed sync attempt.
    async fn complete_failed_sync(
        &mut self,
        sync_attempt: SyncAttempt<T>,
        err: Error,
    ) -> Result<()> {
        if let Some(session) = self.active_sync_sessions.get_mut(&sync_attempt.topic) {
            session.remove(&sync_attempt.peer);
        }

        if let Some(err) = err.downcast_ref() {
            match err {
                SyncAttemptError::Connection => {
                    warn!("sync attempt failed due to connection error");
                    if sync_attempt.attempts <= self.config.max_retry_attempts {
                        self.reschedule_attempt(sync_attempt).await?;
                        return Ok(());
                    } else {
                        self.engine_actor_tx
                            .send(ToEngineActor::SyncFailed {
                                topic: Some(sync_attempt.topic),
                                peer: sync_attempt.peer,
                            })
                            .await?;
                    }
                }
                SyncAttemptError::Sync(err) => {
                    warn!("sync attempt failed: {}", err);
                    self.engine_actor_tx
                        .send(ToEngineActor::SyncFailed {
                            topic: Some(sync_attempt.topic),
                            peer: sync_attempt.peer,
                        })
                        .await?;
                }
            }
        }

        // @TODO(glyph): We may want to maintain a map of failed peer-topic combinations that can
        // be checked against each announcement received by the sync manager. Otherwise we may run
        // into the case where we are repeatedly initiating a sync session with a faulty peer (this
        // would happen every time we receive an announcement, approximately every 2.2 seconds).

        Ok(())
    }

    /// Remove the given topic from the set of active sync sessions for the given peer and add them
    /// to the set of completed sync sessions.
    ///
    /// If resync is active, a timestamp is created to mark the time of sync completion and the
    /// attempt is then pushed to the back of the resync queue.
    async fn complete_successful_sync(&mut self, mut sync_attempt: SyncAttempt<T>) -> Result<()> {
        trace!("complete_successful_sync");
        self.completed_sync_sessions
            .entry(sync_attempt.topic.clone())
            .or_default()
            .insert(sync_attempt.peer);

        if let Some(session) = self.active_sync_sessions.get_mut(&sync_attempt.topic) {
            session.remove(&sync_attempt.peer);
        }

        if self.config.is_resync() {
            trace!("schedule re-sync attempt");
            sync_attempt.completed = Some(Instant::now());
            self.resync_queue.push_back(sync_attempt);
        }

        Ok(())
    }
}
