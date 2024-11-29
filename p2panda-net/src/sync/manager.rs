// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::hash_map::Entry as HashMapEntry;
use std::collections::{HashMap, VecDeque};

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

const FALLBACK_RESYNC_INTERVAL_SEC: u64 = 3600;

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

/// Sync session status.
#[derive(Clone, Debug, Eq, PartialEq)]
enum Status {
    Pending,
    Active,
    Complete(Instant),
    Failed(Instant),
}

// @TODO(glyph): I just noticed that `Scope` is the same as `ToSyncActor`...
/// Sync session scope; defined as a peer-topic combination.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct Scope<T> {
    peer: NodeId,
    topic: T,
}

impl<T> Scope<T> {
    fn new(peer: NodeId, topic: T) -> Self {
        Self { peer, topic }
    }
}

/// Sync session attempt with associated scope and state.
#[derive(Clone, Debug)]
struct Attempt<T> {
    scope: Scope<T>,
    status: Status,
    attempts: u8,
}

impl<T> Attempt<T> {
    fn new(scope: Scope<T>) -> Self {
        Self {
            scope,
            status: Status::Pending,
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

    /// Error occurred while initiating or accepting a sync session.
    #[error(transparent)]
    Sync(#[from] SyncError),
}

/// An API for scheduling outbound connections and sync attempts.
#[derive(Debug)]
pub(crate) struct SyncActor<T> {
    config: SyncConfiguration<T>,
    sessions: HashMap<Scope<T>, Attempt<T>>,
    endpoint: Endpoint,
    engine_actor_tx: Sender<ToEngineActor<T>>,
    inbox: Receiver<ToSyncActor<T>>,
    resync_queue: VecDeque<Attempt<T>>,
    sync_queue_tx: Sender<Attempt<T>>,
    sync_queue_rx: Receiver<Attempt<T>>,
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
            sessions: HashMap::new(),
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
    async fn schedule_attempt(&mut self, attempt: Attempt<T>) -> Result<()> {
        // Only send if the queue is not full; this prevents the possibility of blocking on send.
        if self.sync_queue_tx.capacity() < self.sync_queue_tx.max_capacity() {
            self.sync_queue_tx.send(attempt).await?;
        } else {
            self.sync_queue_tx
                .send_timeout(attempt, self.config.sync_queue_send_timeout)
                .await?;
        }

        Ok(())
    }

    /// Add a peer and topic combination to the sync connection queue, incrementing the number of
    /// previous attempts.
    async fn reschedule_attempt(&mut self, mut attempt: Attempt<T>) -> Result<()> {
        attempt.attempts += 1;
        let _previous_attempt = self
            .sessions
            .insert(attempt.scope.to_owned(), attempt.to_owned());

        self.schedule_attempt(attempt).await?;

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
                Some(attempt) = self.sync_queue_rx.recv() => {
                    match self
                       .connect_and_sync(attempt.scope.clone())
                       .await
                   {
                       Ok(()) => self.complete_successful_sync(attempt).await?,
                       Err(err) => self.complete_failed_sync(attempt, err).await?,
                   }
                },
                // We received a peer-topic announcement from the discovery layer.
                msg = self.inbox.recv() => {
                    let msg = msg.context("sync manager inbox closed")?;
                    let peer = msg.peer;
                    let topic = msg.topic;

                    let scope = Scope::new(peer, topic);

                    // Only schedule an attempt if we're not already tracking sessions for this
                    // scope.
                    if let HashMapEntry::Vacant(entry) = self.sessions.entry(scope.clone()) {
                        let attempt = Attempt::new(scope);
                        entry.insert(attempt.clone());

                        if let Err(err) = self.schedule_attempt(attempt).await {
                            // The attempt will fail if the sync queue is full, indicating that a high
                            // volume of sync sessions are underway. In that case, we drop the attempt
                            // completely. Another attempt will be scheduled when the next announcement of
                            // this peer-topic combination is received from the network-wide gossip
                            // overlay.
                            error!("failed to schedule sync attempt: {}", err)
                        }
                    }
                }
                _ = resync_poll_interval.tick() => {
                    if let Some(attempt) = self.resync_queue.pop_front() {
                        if let Status::Complete(completion) = attempt.status {
                            // Only schedule another attempt if sufficient time has elapsed since
                            // the last successful sync session for this scope.
                            if completion.elapsed() >= resync_interval {
                                trace!("schedule resync attempt {attempt:?}");
                                if let Err(err) = self.schedule_attempt(attempt).await {
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

    /// Attempt to connect with the given peer and initiate a sync session.
    async fn connect_and_sync(&mut self, scope: Scope<T>) -> Result<()> {
        debug!("attempting peer connection for sync");

        if let Some(attempt) = self.sessions.get_mut(&scope) {
            attempt.status = Status::Active
        }

        let peer = scope.peer;
        let topic = scope.topic;

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
    async fn complete_failed_sync(&mut self, attempt: Attempt<T>, err: Error) -> Result<()> {
        if let Some(failed_attempt) = self.sessions.get_mut(&attempt.scope) {
            failed_attempt.status = Status::Failed(Instant::now())
        }

        if let Some(err) = err.downcast_ref() {
            match err {
                // If the sync attempt failed because of a connection error we want to retry up to
                // `max_retry_attempts`. If error occurs after this we simply stop trying without
                // informing the engine as it never knew the attempts were occurring.
                SyncAttemptError::Connection => {
                    warn!("sync attempt failed due to connection error");
                    if attempt.attempts <= self.config.max_retry_attempts {
                        self.reschedule_attempt(attempt).await?;
                        return Ok(());
                    }
                }
                SyncAttemptError::Sync(_) => {
                    self.engine_actor_tx
                        .send(ToEngineActor::SyncFailed {
                            topic: Some(attempt.scope.topic),
                            peer: attempt.scope.peer,
                        })
                        .await?;
                }
            }
        }

        Ok(())
    }

    /// Remove the given topic from the set of active sync sessions for the given peer and add them
    /// to the set of completed sync sessions.
    ///
    /// If resync is active, a timestamp is created to mark the time of sync completion and the
    /// attempt is then pushed to the back of the resync queue.
    async fn complete_successful_sync(&mut self, mut attempt: Attempt<T>) -> Result<()> {
        trace!("complete successful sync");
        attempt.status = Status::Complete(Instant::now());
        let _previous_attempt = self
            .sessions
            .insert(attempt.scope.to_owned(), attempt.to_owned());

        if self.config.is_resync() {
            trace!("schedule re-sync attempt");
            self.resync_queue.push_back(attempt);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6};
    use std::sync::Arc;

    use futures_util::FutureExt;
    use iroh_net::endpoint::TransportConfig;
    use iroh_net::relay::RelayMode;
    use iroh_net::Endpoint;
    use tokio::sync::mpsc;
    use tokio::time::{sleep, Duration};
    use tokio_util::sync::CancellationToken;
    use tracing::warn;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::EnvFilter;

    use crate::engine::ToEngineActor;
    use crate::network::sync_protocols::PingPongProtocol;
    use crate::network::tests::TestTopic;
    use crate::protocols::ProtocolMap;
    use crate::sync::{SyncConnection, SYNC_CONNECTION_ALPN};
    use crate::{ResyncConfiguration, SyncConfiguration};

    use super::{SyncActor, ToSyncActor};

    fn setup_logging() {
        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
            .with(EnvFilter::from_default_env())
            .try_init()
            .ok();
    }

    async fn build_endpoint(port: u16) -> Endpoint {
        let mut transport_config = TransportConfig::default();
        transport_config
            .max_concurrent_bidi_streams(1024u32.into())
            .max_concurrent_uni_streams(0u32.into());

        let socket_address_v4 = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port);
        let socket_address_v6 = SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, port + 1, 0, 0);

        Endpoint::builder()
            //.alpns(vec![SYNC_CONNECTION_ALPN.to_vec()])
            .transport_config(transport_config)
            .relay_mode(RelayMode::Disabled)
            .bind_addr_v4(socket_address_v4)
            .bind_addr_v6(socket_address_v6)
            .bind()
            .await
            .unwrap()
    }

    async fn handle_connection(
        mut connecting: iroh_net::endpoint::Connecting,
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
        setup_logging();

        let test_topic = TestTopic::new("ping_pong");
        let ping_pong = PingPongProtocol {};
        let config_a = SyncConfiguration::new(ping_pong.clone());
        let config_b = config_a.clone();

        let (engine_actor_tx_a, mut engine_actor_rx_a) = mpsc::channel(64);
        let (engine_actor_tx_b, mut engine_actor_rx_b) = mpsc::channel(64);

        let endpoint_a = build_endpoint(2022).await;
        let endpoint_b = build_endpoint(2024).await;

        let mut protocols_a = ProtocolMap::default();
        let sync_handler_a =
            SyncConnection::new(Arc::new(ping_pong.clone()), engine_actor_tx_a.clone());
        protocols_a.insert(SYNC_CONNECTION_ALPN, Arc::new(sync_handler_a));
        let alpns_a = protocols_a.alpns();
        endpoint_a.set_alpns(alpns_a).unwrap();

        let mut protocols_b = ProtocolMap::default();
        let sync_handler_b = SyncConnection::new(Arc::new(ping_pong), engine_actor_tx_b.clone());
        protocols_b.insert(SYNC_CONNECTION_ALPN, Arc::new(sync_handler_b));
        let alpns_b = protocols_b.alpns();
        endpoint_b.set_alpns(alpns_b).unwrap();

        let peer_a = endpoint_a.node_id();
        let peer_b = endpoint_b.node_id();

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

        // Spawn the sync actor for peer a.
        tokio::task::spawn(async move { sync_actor_a.run(shutdown_token_a).await.unwrap() });

        // Spawn the inbound connection handler for peer a.
        tokio::task::spawn(async move {
            if let Some(incoming) = endpoint_a.accept().await {
                if let Ok(connecting) = incoming.accept() {
                    tokio::task::spawn(async move {
                        handle_connection(connecting, Arc::new(protocols_a)).await
                    });
                }
            }
        });

        // Spawn the sync actor for peer b.
        tokio::task::spawn(async move { sync_actor_b.run(shutdown_token_b).await.unwrap() });

        // Spawn the inbound connection handler for peer b.
        tokio::task::spawn(async move {
            if let Some(incoming) = endpoint_b.accept().await {
                if let Ok(connecting) = incoming.accept() {
                    tokio::task::spawn(async move {
                        handle_connection(connecting, Arc::new(protocols_b)).await
                    });
                }
            }
        });

        // Trigger sync session initiation by peer a.
        sync_actor_tx_a
            .send(ToSyncActor {
                peer: peer_b,
                topic: test_topic.clone(),
            })
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

        // @TODO(glyph): Match on the remaining events.
    }

    #[tokio::test]
    async fn second_sync_without_resync() {
        setup_logging();

        let test_topic = TestTopic::new("ping_pong");
        let ping_pong = PingPongProtocol {};
        let config_a = SyncConfiguration::new(ping_pong.clone());
        let config_b = config_a.clone();

        let (engine_actor_tx_a, mut engine_actor_rx_a) = mpsc::channel(64);
        let (engine_actor_tx_b, mut engine_actor_rx_b) = mpsc::channel(64);

        let endpoint_a = build_endpoint(2022).await;
        let endpoint_b = build_endpoint(2024).await;

        let mut protocols_a = ProtocolMap::default();
        let sync_handler_a =
            SyncConnection::new(Arc::new(ping_pong.clone()), engine_actor_tx_a.clone());
        protocols_a.insert(SYNC_CONNECTION_ALPN, Arc::new(sync_handler_a));
        let alpns_a = protocols_a.alpns();
        endpoint_a.set_alpns(alpns_a).unwrap();

        let mut protocols_b = ProtocolMap::default();
        let sync_handler_b = SyncConnection::new(Arc::new(ping_pong), engine_actor_tx_b.clone());
        protocols_b.insert(SYNC_CONNECTION_ALPN, Arc::new(sync_handler_b));
        let alpns_b = protocols_b.alpns();
        endpoint_b.set_alpns(alpns_b).unwrap();

        let peer_a = endpoint_a.node_id();
        let peer_b = endpoint_b.node_id();

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

        // Spawn the sync actor for peer a.
        tokio::task::spawn(async move { sync_actor_a.run(shutdown_token_a).await.unwrap() });

        // Spawn the inbound connection handler for peer a.
        tokio::task::spawn(async move {
            if let Some(incoming) = endpoint_a.accept().await {
                if let Ok(connecting) = incoming.accept() {
                    tokio::task::spawn(async move {
                        handle_connection(connecting, Arc::new(protocols_a)).await
                    });
                }
            }
        });

        // Spawn the sync actor for peer b.
        tokio::task::spawn(async move { sync_actor_b.run(shutdown_token_b).await.unwrap() });

        // Spawn the inbound connection handler for peer b.
        tokio::task::spawn(async move {
            if let Some(incoming) = endpoint_b.accept().await {
                if let Ok(connecting) = incoming.accept() {
                    tokio::task::spawn(async move {
                        handle_connection(connecting, Arc::new(protocols_b)).await
                    });
                }
            }
        });

        // Trigger sync session initiation by peer a.
        //
        // This would occur when the next peer-topic announcement arrived via the network-wide
        // gossip overlay.
        sync_actor_tx_a
            .send(ToSyncActor {
                peer: peer_b,
                topic: test_topic.clone(),
            })
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

        // @TODO(glyph): Match on the remaining events.

        // Now we trigger sync session initiation by peer a for a second time.
        //
        // This emulates the scope being sent to the sync manager via the peer discovery
        // announcement mechanism.
        sync_actor_tx_a
            .send(ToSyncActor {
                peer: peer_b,
                topic: test_topic.clone(),
            })
            .await
            .unwrap();

        // Sleep briefly to ensure time for a potential second session to be initiated.
        sleep(Duration::from_secs(3)).await;

        /* --- PEER A SYNC EVENTS --- */
        /* --- role: initiator    --- */

        assert!(engine_actor_rx_a.recv().now_or_never().is_none());
    }

    #[tokio::test]
    async fn resync() {
        setup_logging();

        let test_topic = TestTopic::new("ping_pong");
        let ping_pong = PingPongProtocol {};
        let resync_config = ResyncConfiguration::new().interval(3).poll_interval(1);
        let config_a = SyncConfiguration::new(ping_pong.clone()).resync(resync_config);
        let config_b = config_a.clone();

        let (engine_actor_tx_a, mut engine_actor_rx_a) = mpsc::channel(64);
        let (engine_actor_tx_b, mut engine_actor_rx_b) = mpsc::channel(64);

        let endpoint_a = build_endpoint(2022).await;
        let endpoint_b = build_endpoint(2024).await;

        let mut protocols_a = ProtocolMap::default();
        let sync_handler_a =
            SyncConnection::new(Arc::new(ping_pong.clone()), engine_actor_tx_a.clone());
        protocols_a.insert(SYNC_CONNECTION_ALPN, Arc::new(sync_handler_a));
        let protocols_a = Arc::new(protocols_a.clone());
        let alpns_a = protocols_a.alpns();
        endpoint_a.set_alpns(alpns_a).unwrap();

        let mut protocols_b = ProtocolMap::default();
        let sync_handler_b = SyncConnection::new(Arc::new(ping_pong), engine_actor_tx_b.clone());
        protocols_b.insert(SYNC_CONNECTION_ALPN, Arc::new(sync_handler_b));
        let protocols_b = Arc::new(protocols_b.clone());
        let alpns_b = protocols_b.alpns();
        endpoint_b.set_alpns(alpns_b).unwrap();

        let peer_a = endpoint_a.node_id();
        let peer_b = endpoint_b.node_id();

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

        // Spawn the sync actor for peer a.
        tokio::task::spawn(async move { sync_actor_a.run(shutdown_token_a).await.unwrap() });

        // Spawn the inbound connection handler for peer a.
        tokio::task::spawn(async move {
            while let Some(incoming) = endpoint_a.accept().await {
                if let Ok(connecting) = incoming.accept() {
                    let protocols_a = protocols_a.clone();
                    tokio::task::spawn(
                        async move { handle_connection(connecting, protocols_a).await },
                    );
                }
            }
        });

        // Spawn the sync actor for peer b.
        tokio::task::spawn(async move { sync_actor_b.run(shutdown_token_b).await.unwrap() });

        // Spawn the inbound connection handler for peer b.
        tokio::task::spawn(async move {
            while let Some(incoming) = endpoint_b.accept().await {
                if let Ok(connecting) = incoming.accept() {
                    let protocols_b = protocols_b.clone();
                    tokio::task::spawn(
                        async move { handle_connection(connecting, protocols_b).await },
                    );
                }
            }
        });

        // Trigger sync session initiation by peer a.
        sync_actor_tx_a
            .send(ToSyncActor {
                peer: peer_b,
                topic: test_topic.clone(),
            })
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
        // `ToSyncActor` event into the peer a sync manager. This proves that our resync logic is
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
