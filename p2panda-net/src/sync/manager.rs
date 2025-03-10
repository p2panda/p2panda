// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::hash_map::Entry as HashMapEntry;
use std::collections::{HashMap, VecDeque};

use anyhow::{Context, Error, Result};
use iroh::Endpoint;
use p2panda_core::PublicKey;
use p2panda_sync::{SyncError, TopicQuery};
use thiserror::Error;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::time::{Duration, Instant, interval};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, warn};

use crate::engine::ToEngineActor;
use crate::from_public_key;
use crate::sync::config::FALLBACK_RESYNC_INTERVAL_SEC;
use crate::sync::{self, SYNC_CONNECTION_ALPN, SyncConfiguration};

/// Events sent to the sync manager.
#[derive(Debug)]
pub enum ToSyncActor<T> {
    /// A new peer-topic combination was discovered.
    Discovery { peer: PublicKey, topic: T },
    /// A major network interface change was detected.
    Reset,
}

impl<T> ToSyncActor<T> {
    pub(crate) fn new_discovery(peer: PublicKey, topic: T) -> Self {
        Self::Discovery { peer, topic }
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

/// Sync session scope; defined as a peer-topic combination.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct Scope<T> {
    peer: PublicKey,
    topic: T,
}

impl<T> Scope<T> {
    fn new(peer: PublicKey, topic: T) -> Self {
        Self { peer, topic }
    }
}

/// Sync session attempt tracker with associated status and number of attempts.
#[derive(Clone, Debug)]
struct Attempt {
    status: Status,
    attempts: u8,
}

impl Attempt {
    fn new() -> Self {
        Self {
            status: Status::Pending,
            attempts: 0,
        }
    }

    fn reset(&mut self) {
        self.status = Status::Pending;
        self.attempts = 0;
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
    sessions: HashMap<Scope<T>, Attempt>,
    endpoint: Endpoint,
    engine_actor_tx: Sender<ToEngineActor<T>>,
    inbox: Receiver<ToSyncActor<T>>,
    resync_queue: VecDeque<Scope<T>>,
    retry_queue: VecDeque<Scope<T>>,
    sync_queue_tx: Sender<Scope<T>>,
    sync_queue_rx: Receiver<Scope<T>>,
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
            sessions: HashMap::new(),
            endpoint,
            engine_actor_tx,
            inbox: sync_manager_rx,
            resync_queue: VecDeque::new(),
            retry_queue: VecDeque::new(),
            sync_queue_tx,
            sync_queue_rx,
        };

        (sync_manager, sync_manager_tx)
    }

    /// The sync connection event loop.
    ///
    /// Listens and responds to three kinds of events:
    ///
    /// - A shutdown signal from the engine
    /// - A new peer-topic combination received from the engine
    /// - A sync attempt pulled from the queue, resulting in a call to `connect_and_sync()`
    /// - A tick of the resync poll interval, resulting in a resync attempt if one is in the queue
    /// - A tick of the retry poll interval, resulting in a retry attempt if one is in the queue
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
        // Define the retry intervals.
        let (mut retry_poll_interval, retry_interval) = (
            interval(self.config.retry_poll_interval),
            self.config.retry_interval,
        );

        loop {
            tokio::select! {
                biased;

                _ = token.cancelled() => {
                    debug!("sync manager received shutdown signal from engine");
                    break;
                }
                msg = self.inbox.recv() => {
                    let msg = msg.context("sync manager inbox closed")?;
                    match msg {
                        // A peer-topic announcement has been received from the discovery layer.
                        ToSyncActor::Discovery { peer, topic } => {
                            let scope = Scope::new(peer, topic);

                            // Only schedule an attempt if we're not already tracking sessions for this
                            // scope.
                            if let HashMapEntry::Vacant(entry) = self.sessions.entry(scope.clone()) {
                                let attempt = Attempt::new();
                                entry.insert(attempt);

                                if let Err(err) = self.schedule_attempt(scope).await {
                                    // The attempt will fail if the sync queue is full, indicating that a high
                                    // volume of sync sessions are underway. In that case, we drop the attempt
                                    // completely. Another attempt will be scheduled when the next announcement of
                                    // this peer-topic combination is received from the network-wide gossip
                                    // overlay.
                                    error!("failed to schedule sync attempt: {}", err)
                                }
                            }
                        },
                        // In the event of a disconnection, two peers who had previously synced may
                        // fall back out of sync. In order to invoke resync upon reconnection, we
                        // reset the status of all sessions and schedule an attempt for each one.
                        // This allows the peers to resync before entering "live mode" (gossip) again.
                        ToSyncActor::Reset => {
                            for attempt in self.sessions.values_mut() {
                                attempt.reset();
                            }

                            for scope in self.sessions.keys() {
                                self.schedule_attempt(scope.clone()).await?;
                            }
                        }
                    }
                }
                Some(scope) = self.sync_queue_rx.recv() => {
                    match self
                       .connect_and_sync(scope.clone())
                       .await
                   {
                       Ok(()) => self.complete_successful_sync(scope).await?,
                       Err(err) => self.complete_failed_sync(scope, err).await?,
                   }
                },
                 _ = resync_poll_interval.tick() => {
                    if let Some(scope) = self.resync_queue.pop_front() {
                        if let Some(attempt) = self.sessions.get(&scope) {
                            if let Status::Complete(completion) = attempt.status {
                                if completion.elapsed() >= resync_interval {
                                    if let Err(err) = self.schedule_attempt(scope).await {
                                        error!("failed to schedule resync attempt: {}", err)
                                    }
                                } else {
                                    self.resync_queue.push_back(scope)
                                }
                            }
                        }
                    }
                }
                _ = retry_poll_interval.tick() => {
                    if let Some(scope) = self.retry_queue.pop_front() {
                        if let Some(attempt) = self.sessions.get(&scope) {
                            if let Status::Failed(failure) = attempt.status {
                                if failure.elapsed() >= retry_interval {
                                    if let Err(err) = self.schedule_attempt(scope).await {
                                        error!("failed to schedule resync attempt: {}", err)
                                    }
                                } else {
                                    self.retry_queue.push_back(scope)
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Schedule a sync attempt for the given scope (peer-topic combination).
    async fn schedule_attempt(&self, scope: Scope<T>) -> Result<()> {
        // Only send if the queue is not full; this prevents the possibility of blocking on send.
        if self.sync_queue_tx.capacity() < self.sync_queue_tx.max_capacity() {
            self.sync_queue_tx.send(scope).await?;
        } else {
            self.sync_queue_tx
                .send_timeout(scope, self.config.sync_queue_send_timeout)
                .await?;
        }

        Ok(())
    }

    /// Attempt to connect with the given peer and initiate a sync session.
    async fn connect_and_sync(&mut self, scope: Scope<T>) -> Result<()> {
        if let Some(attempt) = self.sessions.get_mut(&scope) {
            attempt.status = Status::Active
        }

        let peer = scope.peer;
        let topic = scope.topic;

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

    /// Mark the status of the attempt as `Complete`.
    ///
    /// The attempt is pushed to the back of the resync queue if resync mode is active.
    async fn complete_successful_sync(&mut self, scope: Scope<T>) -> Result<()> {
        if let Some(attempt) = self.sessions.get_mut(&scope) {
            attempt.status = Status::Complete(Instant::now())
        }

        if self.config.is_resync() {
            self.resync_queue.push_back(scope);
        }

        Ok(())
    }

    /// Mark the status of the attempt as `Failed`, increment the attempts counter and inform the
    /// engine of the failure.
    ///
    /// The attempt is pushed to the back of the retry queue if the maximum number of retry
    /// attempts has not been exceeded.
    async fn complete_failed_sync(&mut self, scope: Scope<T>, err: Error) -> Result<()> {
        warn!("sync attempt failed for scope {:?}: {}", scope, err);

        // Inform the engine of the failed attempt so that the gossip buffer counter
        // can be decremented (if one exists).
        self.engine_actor_tx
            .send(ToEngineActor::SyncFailed {
                topic: Some(scope.topic.clone()),
                peer: scope.peer,
            })
            .await?;

        if let Some(attempt) = self.sessions.get_mut(&scope) {
            attempt.status = Status::Failed(Instant::now());
            attempt.attempts += 1;

            if attempt.attempts <= self.config.max_retry_attempts {
                self.retry_queue.push_back(scope);
            }
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
    use p2panda_sync::SyncProtocol;
    use p2panda_sync::test_protocols::{PingPongProtocol, SyncTestTopic as TestTopic};
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
