// SPDX-License-Identifier: MIT OR Apache-2.0

use anyhow::{Context, Result};
use futures_lite::FutureExt;
use iroh::Endpoint;
use netwatch::netmon::Monitor;
use p2panda_core::{PrivateKey, PublicKey};
use p2panda_sync::TopicQuery;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::time::interval;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, warn};

use crate::addrs::{from_node_addr, to_relay_url};
use crate::engine::address_book::AddressBook;
use crate::engine::constants::{
    ANNOUNCE_TOPICS_INTERVAL, JOIN_NETWORK_INTERVAL, JOIN_TOPICS_INTERVAL,
};
use crate::engine::gossip::{GossipActor, ToGossipActor};
use crate::engine::topic_discovery::TopicDiscovery;
use crate::engine::topic_streams::TopicStreams;
use crate::events::SystemEvent;
use crate::network::{FromNetwork, ToNetwork};
use crate::sync::manager::{SyncActor, ToSyncActor};
use crate::{NetworkId, NodeAddress, TopicId, from_public_key, to_public_key};

#[derive(Debug)]
pub enum ToEngineActor<T> {
    AddPeer {
        node_addr: NodeAddress,
    },
    SubscribeEvents {
        reply: oneshot::Sender<broadcast::Receiver<SystemEvent<T>>>,
    },
    KnownPeers {
        reply: oneshot::Sender<Vec<NodeAddress>>,
    },
    SubscribeTopic {
        topic: T,
        from_network_tx: mpsc::Sender<FromNetwork>,
        to_network_rx: mpsc::Receiver<ToNetwork>,
        gossip_ready_tx: oneshot::Sender<()>,
    },
    GossipJoined {
        topic_id: [u8; 32],
        peers: Vec<PublicKey>,
    },
    GossipNeighborUp {
        topic_id: [u8; 32],
        peer: PublicKey,
    },
    GossipNeighborDown {
        topic_id: [u8; 32],
        peer: PublicKey,
    },
    GossipMessage {
        bytes: Vec<u8>,
        delivered_from: PublicKey,
        topic_id: [u8; 32],
    },
    SyncStart {
        topic: Option<T>,
        peer: PublicKey,
    },
    SyncHandshakeSuccess {
        topic: T,
        peer: PublicKey,
    },
    SyncMessage {
        topic: T,
        header: Vec<u8>,
        payload: Option<Vec<u8>>,
        delivered_from: PublicKey,
    },
    SyncDone {
        topic: T,
        peer: PublicKey,
    },
    SyncFailed {
        topic: Option<T>,
        peer: PublicKey,
    },
    Shutdown {
        reply: oneshot::Sender<()>,
    },
}

/// The core event orchestrator of the networking layer.
pub struct EngineActor<T> {
    private_key: PrivateKey,
    address_book: AddressBook,
    endpoint: Endpoint,
    gossip_actor_tx: mpsc::Sender<ToGossipActor>,
    inbox: mpsc::Receiver<ToEngineActor<T>>,
    network_id: NetworkId,
    sync_actor_tx: Option<mpsc::Sender<ToSyncActor<T>>>,
    system_event_tx: Option<broadcast::Sender<SystemEvent<T>>>,
    topic_discovery: TopicDiscovery,
    topic_streams: TopicStreams<T>,
}

impl<T> EngineActor<T>
where
    T: TopicQuery + TopicId + 'static,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        private_key: PrivateKey,
        endpoint: Endpoint,
        address_book: AddressBook,
        inbox: mpsc::Receiver<ToEngineActor<T>>,
        gossip_actor_tx: mpsc::Sender<ToGossipActor>,
        sync_actor_tx: Option<mpsc::Sender<ToSyncActor<T>>>,
        network_id: NetworkId,
        bootstrap: bool,
    ) -> Self {
        let topic_discovery = TopicDiscovery::new(
            network_id,
            gossip_actor_tx.clone(),
            address_book.clone(),
            bootstrap,
        );
        let topic_streams = TopicStreams::new(
            gossip_actor_tx.clone(),
            address_book.clone(),
            sync_actor_tx.clone(),
        );

        Self {
            private_key,
            address_book,
            endpoint,
            gossip_actor_tx,
            inbox,
            network_id,
            sync_actor_tx,
            system_event_tx: None,
            topic_discovery,
            topic_streams,
        }
    }

    /// Runs the sync manager and gossip actor, sets up shutdown handlers and spawns the engine
    /// event loop.
    pub async fn run(
        mut self,
        mut gossip_actor: GossipActor<T>,
        sync_actor: Option<SyncActor<T>>,
    ) -> Result<()> {
        // Used to shutdown the sync manager.
        let shutdown_token = CancellationToken::new();

        if let Some(sync_actor) = sync_actor {
            let shutdown_token = shutdown_token.clone();
            tokio::task::spawn(async move {
                if let Err(err) = sync_actor.run(shutdown_token).await {
                    error!("sync manager failed to run: {err:?}");
                }
            });
        }

        let gossip_handle = tokio::task::spawn(async move {
            if let Err(err) = gossip_actor.run().await {
                error!("gossip recv actor failed: {err:?}");
            }
        });

        // Take oneshot sender from outside API awaited by `shutdown` call and fire it as soon as
        // shutdown completed.
        let shutdown_completed_signal = self.run_inner().await;
        if let Err(err) = self.shutdown().await {
            error!(?err, "error during shutdown");
        }

        shutdown_token.cancel();
        gossip_handle.await?;
        drop(self);

        match shutdown_completed_signal {
            Ok(reply_tx) => {
                reply_tx.send(()).ok();
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    /// Runs the event loop of the engine actor.
    ///
    /// Interval-based timers are used to trigger attempts to join the network-wide and
    /// topic-specific gossip overlays, as well as to announce the locally-subscribed topics.
    async fn run_inner(&mut self) -> Result<oneshot::Sender<()>> {
        let mut join_network_interval = interval(JOIN_NETWORK_INTERVAL);
        let mut join_topics_interval = interval(JOIN_TOPICS_INTERVAL);
        let mut announce_topics_interval = interval(ANNOUNCE_TOPICS_INTERVAL);

        // Setup network monitoring. This allows us to detect major interface changes and reset
        // topic discovery and sync state.
        let network_monitor = Monitor::new().await?;
        let (interface_change_tx, mut interface_change_rx) = mpsc::channel(8);
        let _token = network_monitor
            .subscribe(move |is_major| {
                let interface_change_tx = interface_change_tx.clone();
                async move {
                    interface_change_tx.send(is_major).await.ok();
                }
                .boxed()
            })
            .await?;

        loop {
            tokio::select! {
                biased;
                msg = self.inbox.recv() => {
                    let msg = msg.context("inbox closed")?;
                    match msg {
                        ToEngineActor::Shutdown { reply } => {
                            break Ok(reply);
                        }
                        msg => {
                            if let Err(err) = self.on_actor_message(msg).await {
                                break Err(err);
                            }
                        }
                    }
                },
                // Inform the topic discovery process and sync actor about a major network
                // interface change.
                Some(true) = interface_change_rx.recv() => {
                    // In the event of a disconnection, we will drop out of the topic and
                    // network-wide gossip overlays and may fall out of sync with peers with whom we
                    // had previously synced. Here we inform the sync actor (if one exists) of the
                    // interface change and reset the state of the topic discovery process. This
                    // should result in us reentering the network-wide gossip overlay and resyncing
                    // with our peers before entering "live mode" again.
                    debug!("detected major network interface change");
                    self.topic_discovery.reset_status().await;
                    self.topic_streams.move_joined_to_pending().await;
                    if let Some(sync_actor_tx) = &self.sync_actor_tx {
                        sync_actor_tx.send(ToSyncActor::Reset).await?;
                    }
                    self.gossip_actor_tx.send(ToGossipActor::Reset).await?;
                }
                // Attempt to start topic discovery if it didn't happen yet.
                _ = join_network_interval.tick() => {
                    self.topic_discovery.start().await?;
                },
                // Attempt announcing our currently subscribed topics to other peers.
                _ = announce_topics_interval.tick() => {
                    let my_topic_ids = self.topic_streams.topic_ids();
                    self.topic_discovery.announce(my_topic_ids, &self.private_key).await?;
                },
                // Attempt joining the application's topic gossips if we haven't yet.
                _ = join_topics_interval.tick() => {
                    self.topic_streams.try_join_pending_gossips().await?;
                },
            }
        }
    }

    /// Processes a message received by the actor; these messages represent gossip and sync events.
    async fn on_actor_message(&mut self, msg: ToEngineActor<T>) -> Result<()> {
        match msg {
            ToEngineActor::AddPeer { node_addr } => {
                self.add_peer(node_addr).await?;
            }
            ToEngineActor::SubscribeEvents { reply } => {
                let event_rx = self.events();
                reply.send(event_rx).ok();
            }
            ToEngineActor::KnownPeers { reply } => {
                let list = self.address_book.known_peers().await;
                reply.send(list).ok();
            }
            ToEngineActor::SubscribeTopic {
                topic,
                from_network_tx,
                to_network_rx,
                gossip_ready_tx,
            } => {
                self.on_subscribe(topic, from_network_tx, to_network_rx, gossip_ready_tx)
                    .await?;
            }
            ToEngineActor::GossipJoined { topic_id, peers } => {
                self.on_gossip_joined(topic_id, peers).await?;
            }
            ToEngineActor::GossipNeighborUp { topic_id, peer } => {
                self.on_peer_connected(topic_id, peer).await?;
            }
            ToEngineActor::GossipNeighborDown { topic_id, peer } => {
                self.on_peer_disconnected(topic_id, peer).await?;
            }
            ToEngineActor::GossipMessage {
                bytes,
                delivered_from,
                topic_id,
            } => {
                self.on_gossip_message(bytes, delivered_from, topic_id)
                    .await?;
            }
            ToEngineActor::SyncStart { topic, peer } => {
                self.on_sync_start(topic, peer).await?;
            }
            ToEngineActor::SyncHandshakeSuccess { topic, peer } => {
                self.topic_streams.on_sync_handshake_success(topic, peer);
            }
            ToEngineActor::SyncMessage {
                topic,
                header,
                payload,
                delivered_from,
            } => {
                self.topic_streams
                    .on_sync_message(topic, header, payload, delivered_from)
                    .await?;
            }
            ToEngineActor::SyncDone { topic, peer } => {
                self.on_sync_done(topic, peer).await?;
            }
            ToEngineActor::SyncFailed { topic, peer } => {
                self.on_sync_failed(topic, peer).await?;
            }
            ToEngineActor::Shutdown { .. } => {
                unreachable!("handled in run_inner");
            }
        }

        Ok(())
    }

    /// Add a peer to our address book or updates it's entry.
    ///
    /// Any provided network addresses are registered with the endpoint so that automatic
    /// connection attempts can be made.
    async fn add_peer(&mut self, node_addr: NodeAddress) -> Result<()> {
        let public_key = node_addr.public_key;

        // Make sure the low-level networking endpoint also knows about this address, otherwise
        // connection attempts might fail.
        if self
            .endpoint
            .add_node_addr(from_node_addr(node_addr.clone()))
            .is_err()
        {
            // This can fail if we're trying to add ourselves.
            debug!("tried to add invalid node {public_key} to known peers list");
            return Ok(());
        }

        self.address_book.add_peer(node_addr).await;

        // Hot path: Attempt starting topic discovery as soon as we've learned about at least one
        // peer. If this fails we'll try again soon in our internal loop.
        self.topic_discovery.start().await?;

        Ok(())
    }

    /// Return a receiver for system events.
    fn events(&mut self) -> broadcast::Receiver<SystemEvent<T>> {
        if let Some(event_tx) = &self.system_event_tx {
            event_tx.subscribe()
        } else {
            let (event_tx, event_rx) = broadcast::channel(128);
            self.system_event_tx = Some(event_tx);
            event_rx
        }
    }

    /// Update the join status for the given gossip overlay.
    async fn on_gossip_joined(&mut self, topic_id: [u8; 32], peers: Vec<PublicKey>) -> Result<()> {
        if topic_id == self.network_id {
            self.topic_discovery.on_gossip_joined();
        } else {
            self.topic_streams.on_gossip_joined(topic_id).await;
        }

        if let Some(event_tx) = &self.system_event_tx {
            event_tx.send(SystemEvent::GossipJoined { topic_id, peers })?;
        }

        Ok(())
    }

    /// Register the topic and public key of a peer who just became our direct neighbor in the
    /// gossip overlay.
    ///
    /// Through this we can use gossip algorithms also as an additional "peer discovery" mechanism.
    async fn on_peer_connected(&mut self, topic_id: [u8; 32], peer: PublicKey) -> Result<()> {
        self.address_book.add_topic_id(peer, topic_id).await;

        // At this point we only have the public key of the peer, which is not enough to establish
        // direct connections, luckily iroh has handled storing networking information for us
        // internally already.
        if let Some(info) = self.endpoint.remote_info(from_public_key(peer)) {
            let node_addr = NodeAddress {
                public_key: to_public_key(info.node_id),
                direct_addresses: info.addrs.iter().map(|addr| addr.addr).collect(),
                relay_url: info.relay_url.map(|info| to_relay_url(info.relay_url)),
            };
            self.address_book.add_peer(node_addr).await;
        }

        // Hot path: Some other peer joined, so we send them our "topics of interest", this will
        // hopefully speed up their onboarding process into the network.
        if topic_id == self.network_id {
            let my_topic_ids = self.topic_streams.topic_ids();
            self.topic_discovery
                .announce(my_topic_ids, &self.private_key)
                .await?;
        }

        // Notify any system event subscribers.
        if let Some(event_tx) = &self.system_event_tx {
            event_tx.send(SystemEvent::GossipNeighborUp { topic_id, peer })?;
        }

        Ok(())
    }

    /// The given peer is no longer our direct neighbor in the gossip overlay.
    async fn on_peer_disconnected(&mut self, topic_id: [u8; 32], peer: PublicKey) -> Result<()> {
        // Notify any system event subscribers.
        if let Some(event_tx) = &self.system_event_tx {
            event_tx.send(SystemEvent::GossipNeighborDown { topic_id, peer })?;
        }

        Ok(())
    }

    /// Handle a topic subscription.
    ///
    /// - Mark the given topic as being of interest to our node.
    /// - Attempt to join a gossip overlay for the topic if one has not already been joined.
    /// - Broadcast messages to the gossip overlay.
    /// - Announce our topics of interest to the network.
    async fn on_subscribe(
        &mut self,
        topic: T,
        from_network_tx: mpsc::Sender<FromNetwork>,
        to_network_rx: mpsc::Receiver<ToNetwork>,
        gossip_ready_tx: oneshot::Sender<()>,
    ) -> Result<()> {
        self.topic_streams
            .subscribe(
                topic.clone(),
                from_network_tx,
                to_network_rx,
                gossip_ready_tx,
            )
            .await?;

        // Hot path: Announce our "topics of interest" into the network, hopefully this will speed
        // up finding other peers.
        let my_topic_ids = self.topic_streams.topic_ids();
        self.topic_discovery
            .announce(my_topic_ids, &self.private_key)
            .await?;

        Ok(())
    }

    /// Process sync session starting.
    pub async fn on_sync_start(&mut self, topic: Option<T>, peer: PublicKey) -> Result<()> {
        self.topic_streams.on_sync_start(topic.clone(), peer);

        if let Some(event_tx) = &self.system_event_tx {
            event_tx.send(SystemEvent::SyncStarted { topic, peer })?;
        }

        Ok(())
    }

    /// Process sync session finishing.
    pub async fn on_sync_done(&mut self, topic: T, peer: PublicKey) -> Result<()> {
        self.topic_streams.on_sync_done(topic.clone(), peer).await?;

        // Notify any system event subscribers.
        if let Some(event_tx) = &self.system_event_tx {
            event_tx.send(SystemEvent::SyncDone { topic, peer })?;
        }

        Ok(())
    }

    /// Process sync session failure.
    pub async fn on_sync_failed(&mut self, topic: Option<T>, peer: PublicKey) -> Result<()> {
        self.topic_streams
            .on_sync_failed(topic.clone(), peer)
            .await?;

        if let Some(event_tx) = &self.system_event_tx {
            event_tx.send(SystemEvent::SyncFailed { topic, peer })?;
        }

        Ok(())
    }

    /// Process an inbound message from one of the gossip overlays.
    ///
    /// If the message comes from the "network-wide" gossip overlay (determined by the "network
    /// id"), then it gets handled by the "topic discovery" mechanism. Otherwise it is from a regular,
    /// custom application-related gossip overlay around a "topic id".
    async fn on_gossip_message(
        &mut self,
        bytes: Vec<u8>,
        delivered_from: PublicKey,
        topic_id: [u8; 32],
    ) -> Result<()> {
        if topic_id == self.network_id {
            match self.topic_discovery.on_gossip_message(&bytes).await {
                Ok((topic_ids, peer)) => {
                    self.topic_streams
                        .on_discovered_topic_ids(topic_ids, peer)
                        .await?;

                    if let Some(event_tx) = &self.system_event_tx {
                        event_tx.send(SystemEvent::PeerDiscovered { peer })?;
                    }
                }
                Err(err) => {
                    warn!(
                        "could not parse topic-discovery message from {}: {}",
                        delivered_from, err
                    );
                    return Ok(());
                }
            }
        } else {
            self.topic_streams
                .on_gossip_message(topic_id, bytes, delivered_from)
                .await?;
        }

        Ok(())
    }

    /// Shutdown the engine.
    async fn shutdown(&mut self) -> Result<()> {
        self.gossip_actor_tx
            .send(ToGossipActor::Shutdown)
            .await
            .ok();
        Ok(())
    }
}
