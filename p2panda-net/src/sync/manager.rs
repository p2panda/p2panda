// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet, VecDeque};

use anyhow::{Context, Error, Result};
use iroh::Endpoint;
use p2panda_core::PublicKey;
use p2panda_sync::{SyncError, TopicQuery};
use thiserror::Error;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::time::{Duration, Instant, interval};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, trace, warn};

use crate::engine::ToEngineActor;
use crate::from_public_key;
use crate::sync::{self, SYNC_CONNECTION_ALPN};

use super::SyncConfiguration;

const FALLBACK_RESYNC_INTERVAL_SEC: u64 = 3600;

/// Events sent to the sync manager.
#[derive(Debug)]
pub enum ToSyncActor<T> {
    /// A new peer and topic combination was discovered.
    Discovery { peer: PublicKey, topic: T },
    /// A major network interface change was detected.
    Reset,
}

impl<T> ToSyncActor<T> {
    pub(crate) fn new_discovery(peer: PublicKey, topic: T) -> Self {
        Self::Discovery { peer, topic }
    }
}

#[derive(Debug)]
struct SyncAttempt<T> {
    peer: PublicKey,
    topic: T,
    attempts: u8,
    completed: Option<Instant>,
}

impl<T> SyncAttempt<T> {
    fn new(peer: PublicKey, topic: T) -> Self {
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
    pending_sync_sessions: HashMap<T, HashSet<PublicKey>>,
    active_sync_sessions: HashMap<T, HashSet<PublicKey>>,
    completed_sync_sessions: HashMap<T, HashSet<PublicKey>>,
    endpoint: Endpoint,
    engine_actor_tx: Sender<ToEngineActor<T>>,
    inbox: Receiver<ToSyncActor<T>>,
    resync_queue: VecDeque<SyncAttempt<T>>,
    sync_queue_tx: Sender<SyncAttempt<T>>,
    sync_queue_rx: Receiver<SyncAttempt<T>>,
}

impl<T> SyncActor<T>
where
    T: TopicQuery + 'static,
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

    /// Add a peer and topic combination to the sync connection queue for initial sync.
    async fn schedule_attempt(&mut self, sync_attempt: SyncAttempt<T>) -> Result<()> {
        if self.is_pending(&sync_attempt.peer, &sync_attempt.topic)
            || self.is_active(&sync_attempt.peer, &sync_attempt.topic)
            || self.is_complete(&sync_attempt.peer, &sync_attempt.topic)
        {
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

    /// Add a peer and topic combination to the sync connection queue for resync.
    async fn schedule_resync_attempt(&mut self, sync_attempt: SyncAttempt<T>) -> Result<()> {
        if self.is_pending(&sync_attempt.peer, &sync_attempt.topic)
            || self.is_active(&sync_attempt.peer, &sync_attempt.topic)
        {
            return Ok(());
        }

        self.pending_sync_sessions
            .entry(sync_attempt.topic.clone())
            .or_default()
            .insert(sync_attempt.peer);

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
        self.schedule_attempt(sync_attempt).await?;

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
        let (mut resync_poll_interval, resync_interval) =
            if let Some(ref resync) = self.config.resync {
                (interval(resync.poll_interval), resync.interval)
            } else {
                let one_hour = Duration::from_secs(FALLBACK_RESYNC_INTERVAL_SEC);
                (interval(one_hour), one_hour)
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
                    match msg {
                        ToSyncActor::Discovery { peer, topic } => {
                            let sync_attempt = SyncAttempt::new(peer, topic);

                            if let Err(err) = self.schedule_attempt(sync_attempt).await {
                                // The attempt will fail if the sync queue is full, indicating that a high
                                // volume of sync sessions are underway. In that case, we drop the attempt
                                // completely. Another attempt will be scheduled when the next announcement of
                                // this peer-topic combination is received from the network-wide gossip
                                // overlay.
                                error!("failed to schedule sync attempt: {}", err)
                            }
                        },
                        // In the event of a disconnection, two peers who had previously synced may
                        // fall back out of sync. In order to invoke resync upon reconnection, we
                        // clear the map of completed sync sessions when we detect a major network
                        // interface change. This allows the peers to resync before entering "live
                        // mode" (gossip) again.
                        ToSyncActor::Reset => self.completed_sync_sessions.clear()
                    }
                }
                _ = resync_poll_interval.tick() => {
                    if let Some(attempt) = self.resync_queue.pop_front() {
                        if let Some(completion) = attempt.completed {
                            if completion.elapsed() >= resync_interval {
                                trace!("schedule resync attempt {attempt:?}");
                                if let Err(err) = self.schedule_resync_attempt(attempt).await {
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
    fn is_active(&self, peer: &PublicKey, topic: &T) -> bool {
        if let Some(peers) = self.active_sync_sessions.get(topic) {
            peers.contains(peer)
        } else {
            false
        }
    }

    /// Do we have a complete sync session for the given peer topic combination?
    fn is_complete(&self, peer: &PublicKey, topic: &T) -> bool {
        if let Some(peers) = self.completed_sync_sessions.get(topic) {
            peers.contains(peer)
        } else {
            false
        }
    }

    /// Do we have a pending sync session for the given peer topic combination?
    fn is_pending(&self, peer: &PublicKey, topic: &T) -> bool {
        if let Some(peers) = self.pending_sync_sessions.get(topic) {
            peers.contains(peer)
        } else {
            false
        }
    }

    /// Attempt to connect with the given peer and initiate a sync session.
    async fn connect_and_sync(&mut self, peer: PublicKey, topic: T) -> Result<()> {
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
            .connect(from_public_key(peer), SYNC_CONNECTION_ALPN)
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

        if let Some(err) = err.downcast_ref::<SyncAttemptError>() {
            // If the sync attempt failed for any reason we want to retry up to
            // `max_retry_attempts`. If error occurs after this we simply stop trying without
            // informing the engine as it never knew the attempts were occurring.
            warn!("sync attempt failed: {err}");
            if sync_attempt.attempts <= self.config.max_retry_attempts {
                self.reschedule_attempt(sync_attempt).await?;
                return Ok(());
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

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6};
    use std::sync::Arc;

    use futures_util::FutureExt;
    use iroh::{Endpoint, RelayMode};
    use iroh_quinn::TransportConfig;
    use p2panda_core::PublicKey;
    use p2panda_sync::test_protocols::{PingPongProtocol, SyncTestTopic as TestTopic};
    use p2panda_sync::SyncProtocol;
    use tokio::sync::mpsc;
    use tokio::time::{Duration, sleep};
    use tokio_util::sync::CancellationToken;
    use tracing::warn;

    use crate::engine::ToEngineActor;
    use crate::protocols::ProtocolMap;
    use crate::sync::{SYNC_CONNECTION_ALPN, SyncConnection};
    use crate::{ResyncConfiguration, SyncConfiguration, to_public_key};

    use super::{SyncActor, ToSyncActor};

    async fn build_endpoint(port: u16) -> Endpoint {
        let mut transport_config = TransportConfig::default();
        transport_config
            .max_concurrent_bidi_streams(1024u32.into())
            .max_concurrent_uni_streams(0u32.into());

        let socket_address_v4 = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port);
        let socket_address_v6 = SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, port + 1, 0, 0);

        Endpoint::builder()
            .transport_config(transport_config)
            .relay_mode(RelayMode::Disabled)
            .bind_addr_v4(socket_address_v4)
            .bind_addr_v6(socket_address_v6)
            .bind()
            .await
            .unwrap()
    }

    // Sync actor creation, along with all prerequisite setup steps, to reduce boilerplate
    // duplication in the tests which follow.
    async fn prepare_for_sync<P>(
        protocol: P,
        resync: bool,
    ) -> (
        TestTopic,
        PublicKey,
        SyncActor<TestTopic>,
        mpsc::Sender<ToSyncActor<TestTopic>>,
        Endpoint,
        mpsc::Receiver<ToEngineActor<TestTopic>>,
        ProtocolMap,
        CancellationToken,
        PublicKey,
        SyncActor<TestTopic>,
        Endpoint,
        mpsc::Receiver<ToEngineActor<TestTopic>>,
        ProtocolMap,
        CancellationToken,
    )
    where
        P: for<'a> SyncProtocol<'a, TestTopic> + Clone + 'static,
    {
        let test_topic = TestTopic::new("sync_test");

        let config_a = if resync {
            let resync_config = ResyncConfiguration::new().interval(3).poll_interval(1);
            SyncConfiguration::new(protocol.clone()).resync(resync_config)
        } else {
            SyncConfiguration::new(protocol.clone())
        };
        let config_b = config_a.clone();

        let (engine_actor_tx_a, engine_actor_rx_a) = mpsc::channel(64);
        let (engine_actor_tx_b, engine_actor_rx_b) = mpsc::channel(64);

        let endpoint_a = build_endpoint(2022).await;
        let endpoint_b = build_endpoint(2024).await;

        let mut protocols_a = ProtocolMap::default();
        let sync_handler_a =
            SyncConnection::new(Arc::new(protocol.clone()), engine_actor_tx_a.clone());
        protocols_a.insert(SYNC_CONNECTION_ALPN, Arc::new(sync_handler_a));
        let alpns_a = protocols_a.alpns();
        endpoint_a.set_alpns(alpns_a).unwrap();

        let mut protocols_b = ProtocolMap::default();
        let sync_handler_b = SyncConnection::new(Arc::new(protocol), engine_actor_tx_b.clone());
        protocols_b.insert(SYNC_CONNECTION_ALPN, Arc::new(sync_handler_b));
        let alpns_b = protocols_b.alpns();
        endpoint_b.set_alpns(alpns_b).unwrap();

        let peer_a = to_public_key(endpoint_a.node_id());
        let peer_b = to_public_key(endpoint_b.node_id());

        let peer_addr_a = endpoint_a.node_addr().await.unwrap();
        let peer_addr_b = endpoint_b.node_addr().await.unwrap();

        endpoint_a.add_node_addr(peer_addr_b).unwrap();
        endpoint_b.add_node_addr(peer_addr_a).unwrap();

        let (sync_actor_a, sync_actor_tx_a) =
            SyncActor::new(config_a, endpoint_a.clone(), engine_actor_tx_a);
        let (sync_actor_b, _sync_actor_tx_b) =
            SyncActor::new(config_b, endpoint_b.clone(), engine_actor_tx_b);

        let shutdown_token_a = CancellationToken::new();
        let shutdown_token_b = CancellationToken::new();

        (
            test_topic,
            peer_a,
            sync_actor_a,
            sync_actor_tx_a,
            endpoint_a,
            engine_actor_rx_a,
            protocols_a,
            shutdown_token_a,
            peer_b,
            sync_actor_b,
            endpoint_b,
            engine_actor_rx_b,
            protocols_b,
            shutdown_token_b,
        )
    }

    async fn handle_connection(
        mut connecting: iroh::endpoint::Connecting,
        protocols: Arc<ProtocolMap>,
    ) {
        let alpn = match connecting.alpn().await {
            Ok(alpn) => alpn,
            Err(err) => {
                warn!("ignoring connection: invalid handshake: {:?}", err);
                return;
            }
        };
        let Some(handler) = protocols.get(&alpn) else {
            warn!("ignoring connection: unsupported alpn protocol");
            return;
        };
        if let Err(err) = handler.accept(connecting).await {
            warn!("handling incoming connection ended with error: {err}");
        }
    }

    #[tokio::test]
    async fn single_sync() {
        let protocol = PingPongProtocol {};

        let (
            test_topic,
            peer_a,
            sync_actor_a,
            sync_actor_tx_a,
            endpoint_a,
            mut engine_actor_rx_a,
            protocols_a,
            shutdown_token_a,
            peer_b,
            sync_actor_b,
            endpoint_b,
            mut engine_actor_rx_b,
            protocols_b,
            shutdown_token_b,
        ) = prepare_for_sync(protocol, false).await;

        // Spawn the sync actor for peer A.
        tokio::task::spawn(async move { sync_actor_a.run(shutdown_token_a).await.unwrap() });

        // Spawn the inbound connection handler for peer A.
        tokio::task::spawn(async move {
            if let Some(incoming) = endpoint_a.accept().await {
                if let Ok(connecting) = incoming.accept() {
                    tokio::task::spawn(async move {
                        handle_connection(connecting, Arc::new(protocols_a)).await
                    });
                }
            }
        });

        // Spawn the sync actor for peer B.
        tokio::task::spawn(async move { sync_actor_b.run(shutdown_token_b).await.unwrap() });

        // Spawn the inbound connection handler for peer B.
        tokio::task::spawn(async move {
            if let Some(incoming) = endpoint_b.accept().await {
                if let Ok(connecting) = incoming.accept() {
                    tokio::task::spawn(async move {
                        handle_connection(connecting, Arc::new(protocols_b)).await
                    });
                }
            }
        });

        // Trigger sync session initiation by peer A.
        sync_actor_tx_a
            .send(ToSyncActor::new_discovery(peer_b, test_topic.clone()))
            .await
            .unwrap();

        /* --- PEER A SYNC EVENTS --- */
        /* --- role: initiator    --- */

        // Receive `SyncStart`.
        let Some(ToEngineActor::SyncStart { topic, peer }) = engine_actor_rx_a.recv().await else {
            panic!("expected to receive SyncStart on engine actor receiver for peer a")
        };
        assert_eq!(topic, Some(test_topic.to_owned()));
        assert_eq!(peer, peer_b);

        // Receive `SyncHandshakeSuccess`.
        let Some(ToEngineActor::SyncHandshakeSuccess { topic: _, peer: _ }) =
            engine_actor_rx_a.recv().await
        else {
            panic!("expected to receive SyncHandshakeSuccess on engine actor receiver for peer a")
        };

        // Receive `SyncMessage`.
        let Some(ToEngineActor::SyncMessage {
            topic: _,
            header: _,
            payload: _,
            delivered_from: _,
        }) = engine_actor_rx_a.recv().await
        else {
            panic!("expected to receive SyncMessage on engine actor receiver for peer a")
        };

        // Receive `SyncDone`.
        let Some(ToEngineActor::SyncDone { topic: _, peer: _ }) = engine_actor_rx_a.recv().await
        else {
            panic!("expected to receive SyncDone on engine actor receiver for peer a")
        };

        /* --- PEER B SYNC EVENTS --- */
        /* --- role: acceptor     --- */

        // Receive `SyncStart`.
        let Some(ToEngineActor::SyncStart { topic, peer }) = engine_actor_rx_b.recv().await else {
            panic!("expected to receive SyncStart on engine actor receiver for peer a")
        };
        assert_eq!(topic, None);
        assert_eq!(peer, peer_a);

        // Receive `SyncHandshakeSuccess`.
        let Some(ToEngineActor::SyncHandshakeSuccess { topic: _, peer: _ }) =
            engine_actor_rx_b.recv().await
        else {
            panic!("expected to receive SyncHandshakeSuccess on engine actor receiver for peer a")
        };

        // Receive `SyncMessage`.
        let Some(ToEngineActor::SyncMessage {
            topic: _,
            header: _,
            payload: _,
            delivered_from: _,
        }) = engine_actor_rx_b.recv().await
        else {
            panic!("expected to receive SyncMessage on engine actor receiver for peer b")
        };

        // Receive `SyncDone`.
        let Some(ToEngineActor::SyncDone { topic: _, peer: _ }) = engine_actor_rx_b.recv().await
        else {
            panic!("expected to receive SyncDone on engine actor receiver for peer b")
        };
    }

    #[tokio::test]
    async fn second_sync_without_resync() {
        let protocol = PingPongProtocol {};

        let (
            test_topic,
            peer_a,
            sync_actor_a,
            sync_actor_tx_a,
            endpoint_a,
            mut engine_actor_rx_a,
            protocols_a,
            shutdown_token_a,
            peer_b,
            sync_actor_b,
            endpoint_b,
            mut engine_actor_rx_b,
            protocols_b,
            shutdown_token_b,
        ) = prepare_for_sync(protocol, false).await;

        // Spawn the sync actor for peer A.
        tokio::task::spawn(async move { sync_actor_a.run(shutdown_token_a).await.unwrap() });

        // Spawn the inbound connection handler for peer A.
        tokio::task::spawn(async move {
            if let Some(incoming) = endpoint_a.accept().await {
                if let Ok(connecting) = incoming.accept() {
                    tokio::task::spawn(async move {
                        handle_connection(connecting, Arc::new(protocols_a)).await
                    });
                }
            }
        });

        // Spawn the sync actor for peer B.
        tokio::task::spawn(async move { sync_actor_b.run(shutdown_token_b).await.unwrap() });

        // Spawn the inbound connection handler for peer B.
        tokio::task::spawn(async move {
            if let Some(incoming) = endpoint_b.accept().await {
                if let Ok(connecting) = incoming.accept() {
                    tokio::task::spawn(async move {
                        handle_connection(connecting, Arc::new(protocols_b)).await
                    });
                }
            }
        });

        // Trigger sync session initiation by peer A.
        //
        // This would occur when the next peer-topic announcement arrived via the network-wide
        // gossip overlay.
        sync_actor_tx_a
            .send(ToSyncActor::new_discovery(peer_b, test_topic.clone()))
            .await
            .unwrap();

        /* --- PEER A SYNC EVENTS --- */
        /* --- role: initiator    --- */

        // Receive `SyncStart`.
        let Some(ToEngineActor::SyncStart { topic, peer }) = engine_actor_rx_a.recv().await else {
            panic!("expected to receive SyncStart on engine actor receiver for peer a")
        };
        assert_eq!(topic, Some(test_topic.to_owned()));
        assert_eq!(peer, peer_b);

        // Receive `SyncHandshakeSuccess`.
        let Some(ToEngineActor::SyncHandshakeSuccess { topic: _, peer: _ }) =
            engine_actor_rx_a.recv().await
        else {
            panic!("expected to receive SyncHandshakeSuccess on engine actor receiver for peer a")
        };

        // Receive `SyncMessage`.
        let Some(ToEngineActor::SyncMessage {
            topic: _,
            header: _,
            payload: _,
            delivered_from: _,
        }) = engine_actor_rx_a.recv().await
        else {
            panic!("expected to receive SyncMessage on engine actor receiver for peer a")
        };

        // Receive `SyncDone`.
        let Some(ToEngineActor::SyncDone { topic: _, peer: _ }) = engine_actor_rx_a.recv().await
        else {
            panic!("expected to receive SyncDone on engine actor receiver for peer a")
        };

        /* --- PEER B SYNC EVENTS --- */
        /* --- role: acceptor     --- */

        // Receive `SyncStart`.
        let Some(ToEngineActor::SyncStart { topic, peer }) = engine_actor_rx_b.recv().await else {
            panic!("expected to receive SyncStart on engine actor receiver for peer a")
        };
        assert_eq!(topic, None);
        assert_eq!(peer, peer_a);

        // Receive `SyncHandshakeSuccess`.
        let Some(ToEngineActor::SyncHandshakeSuccess { topic: _, peer: _ }) =
            engine_actor_rx_b.recv().await
        else {
            panic!("expected to receive SyncHandshakeSuccess on engine actor receiver for peer a")
        };

        // Receive `SyncMessage`.
        let Some(ToEngineActor::SyncMessage {
            topic: _,
            header: _,
            payload: _,
            delivered_from: _,
        }) = engine_actor_rx_b.recv().await
        else {
            panic!("expected to receive SyncMessage on engine actor receiver for peer b")
        };

        // Receive `SyncDone`.
        let Some(ToEngineActor::SyncDone { topic: _, peer: _ }) = engine_actor_rx_b.recv().await
        else {
            panic!("expected to receive SyncDone on engine actor receiver for peer b")
        };

        // Now we trigger sync session initiation by peer A for a second time.
        //
        // This emulates the scope being sent to the sync manager via the peer discovery
        // announcement mechanism.
        sync_actor_tx_a
            .send(ToSyncActor::new_discovery(peer_b, test_topic.clone()))
            .await
            .unwrap();

        // Sleep briefly to ensure time for a potential second session to be initiated.
        sleep(Duration::from_secs(3)).await;

        /* --- PEER A SYNC EVENTS --- */
        /* --- role: initiator    --- */

        // No further messages should be received, since the first session completed successfully
        // and resync was not configured.
        assert!(engine_actor_rx_a.recv().now_or_never().is_none());
    }

    #[tokio::test]
    async fn second_sync_after_reset() {
        let protocol = PingPongProtocol {};

        let (
            test_topic,
            peer_a,
            sync_actor_a,
            sync_actor_tx_a,
            endpoint_a,
            mut engine_actor_rx_a,
            protocols_a,
            shutdown_token_a,
            peer_b,
            sync_actor_b,
            endpoint_b,
            mut engine_actor_rx_b,
            protocols_b,
            shutdown_token_b,
        ) = prepare_for_sync(protocol, false).await;

        // Spawn the sync actor for peer A.
        tokio::task::spawn(async move { sync_actor_a.run(shutdown_token_a).await.unwrap() });

        // Spawn the inbound connection handler for peer A.
        tokio::task::spawn(async move {
            while let Some(incoming) = endpoint_a.accept().await {
                if let Ok(connecting) = incoming.accept() {
                    let protocols_a = protocols_a.clone();
                    tokio::task::spawn(async move {
                        handle_connection(connecting, protocols_a.into()).await
                    });
                }
            }
        });

        // Spawn the sync actor for peer B.
        tokio::task::spawn(async move { sync_actor_b.run(shutdown_token_b).await.unwrap() });

        // Spawn the inbound connection handler for peer B.
        tokio::task::spawn(async move {
            while let Some(incoming) = endpoint_b.accept().await {
                if let Ok(connecting) = incoming.accept() {
                    let protocols_b = protocols_b.clone();
                    tokio::task::spawn(async move {
                        handle_connection(connecting, protocols_b.into()).await
                    });
                }
            }
        });

        // Trigger sync session initiation by peer A.
        sync_actor_tx_a
            .send(ToSyncActor::new_discovery(peer_b, test_topic.clone()))
            .await
            .unwrap();

        /* --- PEER A SYNC EVENTS --- */
        /* --- role: initiator    --- */
        /* --- initial session    --- */

        // Receive `SyncStart`.
        let Some(ToEngineActor::SyncStart { topic, peer }) = engine_actor_rx_a.recv().await else {
            panic!("expected to receive SyncStart on engine actor receiver for peer a")
        };
        assert_eq!(topic, Some(test_topic.to_owned()));
        assert_eq!(peer, peer_b);

        // Receive `SyncStart`.
        let Some(ToEngineActor::SyncStart { topic, peer }) = engine_actor_rx_b.recv().await else {
            panic!("expected to receive SyncStart on engine actor receiver for peer a")
        };
        assert_eq!(topic, None);
        assert_eq!(peer, peer_a);

        // Receive `SyncHandshakeSuccess`.
        let Some(ToEngineActor::SyncHandshakeSuccess { topic: _, peer: _ }) =
            engine_actor_rx_a.recv().await
        else {
            panic!("expected to receive SyncHandshakeSuccess on engine actor receiver for peer a")
        };

        // Receive `SyncMessage`.
        let Some(ToEngineActor::SyncMessage {
            topic: _,
            header: _,
            payload: _,
            delivered_from: _,
        }) = engine_actor_rx_a.recv().await
        else {
            panic!("expected to receive SyncMessage on engine actor receiver for peer a")
        };

        // Receive `SyncDone`.
        let Some(ToEngineActor::SyncDone { topic: _, peer: _ }) = engine_actor_rx_a.recv().await
        else {
            panic!("expected to receive SyncDone on engine actor receiver for peer a")
        };

        // Trigger reset of sync session completed state for peer A.
        //
        // This would occur when a major network interface change is detected.
        sync_actor_tx_a.send(ToSyncActor::Reset).await.unwrap();

        // Trigger sync session initiation by peer A.
        sync_actor_tx_a
            .send(ToSyncActor::new_discovery(peer_b, test_topic.clone()))
            .await
            .unwrap();

        /* --- PEER A SYNC EVENTS --- */
        /* --- role: initiator    --- */
        /* --- resync session     --- */

        // We expect the full sync cycle to be repeated when the second discovery announcement
        // event is received. This proves that our reset logic is successfully clearing the
        // `completed_sync_sessions` map and allowing a second sync session.

        // Receive `SyncStart`.
        let Some(ToEngineActor::SyncStart { topic, peer }) = engine_actor_rx_a.recv().await else {
            panic!("expected to receive SyncStart on engine actor receiver for peer a")
        };
        assert_eq!(topic, Some(test_topic.to_owned()));
        assert_eq!(peer, peer_b);

        // Receive `SyncHandshakeSuccess`.
        let Some(ToEngineActor::SyncHandshakeSuccess { topic: _, peer: _ }) =
            engine_actor_rx_a.recv().await
        else {
            panic!("expected to receive SyncHandshakeSuccess on engine actor receiver for peer a")
        };

        // Receive `SyncMessage`.
        let Some(ToEngineActor::SyncMessage {
            topic: _,
            header: _,
            payload: _,
            delivered_from: _,
        }) = engine_actor_rx_a.recv().await
        else {
            panic!("expected to receive SyncMessage on engine actor receiver for peer a")
        };

        // Receive `SyncDone`.
        let Some(ToEngineActor::SyncDone { topic: _, peer: _ }) = engine_actor_rx_a.recv().await
        else {
            panic!("expected to receive SyncDone on engine actor receiver for peer a")
        };
    }

    #[tokio::test]
    async fn resync() {
        let protocol = PingPongProtocol {};

        let (
            test_topic,
            peer_a,
            sync_actor_a,
            sync_actor_tx_a,
            endpoint_a,
            mut engine_actor_rx_a,
            protocols_a,
            shutdown_token_a,
            peer_b,
            sync_actor_b,
            endpoint_b,
            mut engine_actor_rx_b,
            protocols_b,
            shutdown_token_b,
        ) = prepare_for_sync(protocol, true).await;

        // Spawn the sync actor for peer A.
        tokio::task::spawn(async move { sync_actor_a.run(shutdown_token_a).await.unwrap() });

        // Spawn the inbound connection handler for peer A.
        tokio::task::spawn(async move {
            while let Some(incoming) = endpoint_a.accept().await {
                if let Ok(connecting) = incoming.accept() {
                    let protocols_a = protocols_a.clone();
                    tokio::task::spawn(async move {
                        handle_connection(connecting, protocols_a.into()).await
                    });
                }
            }
        });

        // Spawn the sync actor for peer B.
        tokio::task::spawn(async move { sync_actor_b.run(shutdown_token_b).await.unwrap() });

        // Spawn the inbound connection handler for peer B.
        tokio::task::spawn(async move {
            while let Some(incoming) = endpoint_b.accept().await {
                if let Ok(connecting) = incoming.accept() {
                    let protocols_b = protocols_b.clone();
                    tokio::task::spawn(async move {
                        handle_connection(connecting, protocols_b.into()).await
                    });
                }
            }
        });

        // Trigger sync session initiation by peer A.
        sync_actor_tx_a
            .send(ToSyncActor::new_discovery(peer_b, test_topic.clone()))
            .await
            .unwrap();

        /* --- PEER A SYNC EVENTS --- */
        /* --- role: initiator    --- */
        /* --- initial session    --- */

        // Receive `SyncStart`.
        let Some(ToEngineActor::SyncStart { topic, peer }) = engine_actor_rx_a.recv().await else {
            panic!("expected to receive SyncStart on engine actor receiver for peer a")
        };
        assert_eq!(topic, Some(test_topic.to_owned()));
        assert_eq!(peer, peer_b);

        // Receive `SyncStart`.
        let Some(ToEngineActor::SyncStart { topic, peer }) = engine_actor_rx_b.recv().await else {
            panic!("expected to receive SyncStart on engine actor receiver for peer a")
        };
        assert_eq!(topic, None);
        assert_eq!(peer, peer_a);

        // Receive `SyncHandshakeSuccess`.
        let Some(ToEngineActor::SyncHandshakeSuccess { topic: _, peer: _ }) =
            engine_actor_rx_a.recv().await
        else {
            panic!("expected to receive SyncHandshakeSuccess on engine actor receiver for peer a")
        };

        // Receive `SyncMessage`.
        let Some(ToEngineActor::SyncMessage {
            topic: _,
            header: _,
            payload: _,
            delivered_from: _,
        }) = engine_actor_rx_a.recv().await
        else {
            panic!("expected to receive SyncMessage on engine actor receiver for peer a")
        };

        // Receive `SyncDone`.
        let Some(ToEngineActor::SyncDone { topic: _, peer: _ }) = engine_actor_rx_a.recv().await
        else {
            panic!("expected to receive SyncDone on engine actor receiver for peer a")
        };

        /* --- PEER A SYNC EVENTS --- */
        /* --- role: initiator    --- */
        /* --- resync session     --- */

        // We expect the full sync cycle to be repeated, even though we only sent one initial
        // `ToSyncActor` event into the peer A sync manager. This proves that our resync logic is
        // successfully initiating a second sync session.

        // Receive `SyncStart`.
        let Some(ToEngineActor::SyncStart { topic, peer }) = engine_actor_rx_a.recv().await else {
            panic!("expected to receive SyncStart on engine actor receiver for peer a")
        };
        assert_eq!(topic, Some(test_topic.to_owned()));
        assert_eq!(peer, peer_b);

        // Receive `SyncHandshakeSuccess`.
        let Some(ToEngineActor::SyncHandshakeSuccess { topic: _, peer: _ }) =
            engine_actor_rx_a.recv().await
        else {
            panic!("expected to receive SyncHandshakeSuccess on engine actor receiver for peer a")
        };

        // Receive `SyncMessage`.
        let Some(ToEngineActor::SyncMessage {
            topic: _,
            header: _,
            payload: _,
            delivered_from: _,
        }) = engine_actor_rx_a.recv().await
        else {
            panic!("expected to receive SyncMessage on engine actor receiver for peer a")
        };

        // Receive `SyncDone`.
        let Some(ToEngineActor::SyncDone { topic: _, peer: _ }) = engine_actor_rx_a.recv().await
        else {
            panic!("expected to receive SyncDone on engine actor receiver for peer a")
        };
    }
}
