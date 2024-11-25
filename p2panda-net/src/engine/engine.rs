// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::{Context, Result};
use iroh_net::key::{PublicKey, SecretKey};
use iroh_net::{Endpoint, NodeAddr, NodeId};
use p2panda_sync::Topic;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::time::interval;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, warn};

use crate::engine::address_book::AddressBook;
use crate::engine::constants::{
    ANNOUNCE_TOPICS_INTERVAL, JOIN_NETWORK_INTERVAL, JOIN_TOPICS_INTERVAL,
};
use crate::engine::gossip::{GossipActor, ToGossipActor};
use crate::engine::topic_discovery::TopicDiscovery;
use crate::engine::topic_streams::TopicStreams;
use crate::network::{FromNetwork, ToNetwork};
use crate::sync::manager::{SyncActor, ToSyncActor};
use crate::{NetworkId, TopicId};

#[derive(Debug)]
pub enum ToEngineActor<T> {
    AddPeer {
        node_addr: NodeAddr,
    },
    KnownPeers {
        reply: oneshot::Sender<Vec<NodeAddr>>,
    },
    Subscribe {
        topic: T,
        from_network_tx: broadcast::Sender<FromNetwork>,
        to_network_rx: mpsc::Receiver<ToNetwork>,
        gossip_ready_tx: oneshot::Sender<()>,
    },
    GossipJoined {
        topic_id: [u8; 32],
    },
    GossipNeighborUp {
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
        peer: PublicKey,
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
    secret_key: SecretKey,
    address_book: AddressBook,
    endpoint: Endpoint,
    gossip_actor_tx: mpsc::Sender<ToGossipActor>,
    inbox: mpsc::Receiver<ToEngineActor<T>>,
    network_id: NetworkId,
    topic_discovery: TopicDiscovery,
    topic_streams: TopicStreams<T>,
}

impl<T> EngineActor<T>
where
    T: Topic + TopicId + 'static,
{
    pub fn new(
        secret_key: SecretKey,
        endpoint: Endpoint,
        address_book: AddressBook,
        inbox: mpsc::Receiver<ToEngineActor<T>>,
        gossip_actor_tx: mpsc::Sender<ToGossipActor>,
        sync_actor_tx: Option<mpsc::Sender<ToSyncActor<T>>>,
        network_id: NetworkId,
    ) -> Self {
        let topic_discovery =
            TopicDiscovery::new(network_id, gossip_actor_tx.clone(), address_book.clone());
        let topic_streams =
            TopicStreams::new(gossip_actor_tx.clone(), address_book.clone(), sync_actor_tx);

        Self {
            secret_key,
            address_book,
            endpoint,
            gossip_actor_tx,
            inbox,
            network_id,
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
                // Attempt to start topic discovery if it didn't happen yet.
                _ = join_network_interval.tick() => {
                    self.topic_discovery.start().await?;
                },
                // Attempt announcing our currently subscribed topics to other peers.
                _ = announce_topics_interval.tick() => {
                    let my_topic_ids = self.topic_streams.topic_ids();
                    self.topic_discovery.announce(my_topic_ids, &self.secret_key).await?;
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
            ToEngineActor::KnownPeers { reply } => {
                let list = self.address_book.known_peers().await;
                reply.send(list).ok();
            }
            ToEngineActor::Subscribe {
                topic,
                from_network_tx,
                to_network_rx,
                gossip_ready_tx,
            } => {
                self.on_subscribe(topic, from_network_tx, to_network_rx, gossip_ready_tx)
                    .await?;
            }
            ToEngineActor::GossipJoined { topic_id } => {
                self.on_gossip_joined(topic_id).await;
            }
            ToEngineActor::GossipNeighborUp { topic_id, peer } => {
                self.on_peer_joined(topic_id, peer).await?;
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
                self.topic_streams.on_sync_start(topic, peer);
            }
            ToEngineActor::SyncHandshakeSuccess { topic, peer } => {
                self.topic_streams.on_sync_handshake_success(topic, peer);
            }
            ToEngineActor::SyncMessage {
                topic,
                header,
                payload,
                peer,
            } => {
                self.topic_streams
                    .on_sync_message(topic, header, payload, peer)?;
            }
            ToEngineActor::SyncDone { topic, peer } => {
                self.topic_streams.on_sync_done(topic, peer).await?;
            }
            ToEngineActor::SyncFailed { topic, peer } => {
                self.topic_streams.on_sync_failed(topic, peer).await?;
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
    async fn add_peer(&mut self, node_addr: NodeAddr) -> Result<()> {
        let node_id = node_addr.node_id;

        // Make sure the low-level networking endpoint also knows about this address, otherwise
        // connection attempts might fail.
        if self.endpoint.add_node_addr(node_addr.clone()).is_err() {
            // This can fail if we're trying to add ourselves.
            debug!("tried to add invalid node {node_id} to known peers list");
            return Ok(());
        }

        self.address_book.add_peer(node_addr).await;

        // Hot path: Attempt starting topic discovery as soon as we've learned about at least one
        // peer. If this fails we'll try again soon in our internal loop.
        self.topic_discovery.start().await?;

        Ok(())
    }

    /// Update the join status for the given gossip overlay.
    async fn on_gossip_joined(&mut self, topic_id: [u8; 32]) {
        if topic_id == self.network_id {
            self.topic_discovery.on_gossip_joined();
        } else {
            self.topic_streams.on_gossip_joined(topic_id).await;
        }
    }

    /// Register the topic and public key of a peer who just became our direct neighbor in the
    /// gossip overlay.
    ///
    /// Through this we can use gossip algorithms also as an additional "peer discovery" mechanism.
    async fn on_peer_joined(&mut self, topic_id: [u8; 32], node_id: NodeId) -> Result<()> {
        // At this point we only have the public key of the peer, which is not enough to establish
        // direct connections, luckily iroh handled storing networking information for us
        // internally already.
        self.address_book.add_topic_id(node_id, topic_id).await;

        // Hot path: Some other peer joined, so we send them our "topics of interest", this will
        // hopefully speed up their onboarding process into the network.
        if topic_id == self.network_id {
            let my_topic_ids = self.topic_streams.topic_ids();
            self.topic_discovery
                .announce(my_topic_ids, &self.secret_key)
                .await?;
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
        from_network_tx: broadcast::Sender<FromNetwork>,
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
            .announce(my_topic_ids, &self.secret_key)
            .await?;

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
            match self
                .topic_discovery
                .on_gossip_message(&bytes)
                .await
            {
                Ok((topic_ids, node_id)) => {
                    self.topic_streams
                        .on_discovered_topic_ids(topic_ids, node_id)
                        .await?;
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
