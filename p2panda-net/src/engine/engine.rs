// SPDX-License-Identifier: AGPL-3.0-or-later

use std::time::Duration;

use anyhow::{Context, Result};
use iroh_net::key::PublicKey;
use iroh_net::{Endpoint, NodeAddr};
use p2panda_sync::Topic;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::time::interval;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, warn};

use crate::engine::gossip::{GossipActor, ToGossipActor};
use crate::engine::gossip_buffer::GossipBuffer;
use crate::engine::message::NetworkMessage;
use crate::engine::peer_map::PeerMap;
use crate::engine::topic_map::TopicMap;
use crate::network::{FromNetwork, ToNetwork};
use crate::sync::manager::{SyncManager, ToSyncActor};
use crate::{FromBytes, NetworkId, ToBytes, TopicId};

/// Maximum size of random sample set when choosing peers to join gossip overlay.
///
/// The larger the number the less likely joining the gossip will fail as we get more chances to
/// establish connections. As soon as we've joined the gossip we will learn about more peers.
const JOIN_PEERS_SAMPLE_LEN: usize = 7;

/// Frequency of attempts to join the network-wide gossip overlay.
const JOIN_NETWORK_INTERVAL: Duration = Duration::from_millis(900);

/// Frequency of locally-subscribed topic announcements (to network peers).
const ANNOUNCE_TOPICS_INTERVAL: Duration = Duration::from_millis(2200);

/// Frequency of attempts to join gossip overlays for locally-subscribed topics.
const JOIN_TOPICS_INTERVAL: Duration = Duration::from_millis(1200);

pub enum ToEngineActor<T> {
    AddPeer {
        node_addr: NodeAddr,
    },
    NeighborUp {
        topic_id: [u8; 32],
        peer: PublicKey,
    },
    Subscribe {
        topic: T,
        from_network_tx: broadcast::Sender<FromNetwork>,
        to_network_rx: mpsc::Receiver<ToNetwork>,
        gossip_ready_tx: oneshot::Sender<()>,
    },
    Received {
        bytes: Vec<u8>,
        delivered_from: PublicKey,
        topic_id: [u8; 32],
    },
    SyncHandshakeSuccess {
        peer: PublicKey,
        topic_id: [u8; 32],
    },
    SyncMessage {
        header: Vec<u8>,
        payload: Option<Vec<u8>>,
        delivered_from: PublicKey,
        topic_id: [u8; 32],
    },
    SyncDone {
        peer: PublicKey,
        topic_id: [u8; 32],
    },
    Shutdown {
        reply: oneshot::Sender<()>,
    },
    TopicJoined {
        topic_id: [u8; 32],
    },
    KnownPeers {
        reply: oneshot::Sender<Result<Vec<NodeAddr>>>,
    },
}

/// The core event orchestrator of the networking layer.
pub struct EngineActor<T> {
    endpoint: Endpoint,
    gossip_actor_tx: mpsc::Sender<ToGossipActor>,
    gossip_buffer: GossipBuffer,
    inbox: mpsc::Receiver<ToEngineActor<T>>,
    network_id: NetworkId,
    network_joined: bool,
    network_joined_pending: bool,
    peers: PeerMap,
    sync_actor_tx: Option<mpsc::Sender<ToSyncActor<T>>>,
    topics: TopicMap<T>,
}

impl<T> EngineActor<T>
where
    T: Topic + TopicId + 'static,
{
    pub fn new(
        endpoint: Endpoint,
        inbox: mpsc::Receiver<ToEngineActor<T>>,
        gossip_actor_tx: mpsc::Sender<ToGossipActor>,
        sync_actor_tx: Option<mpsc::Sender<ToSyncActor<T>>>,
        network_id: NetworkId,
    ) -> Self {
        Self {
            endpoint,
            gossip_actor_tx,
            sync_actor_tx,
            inbox,
            network_id,
            network_joined: false,
            network_joined_pending: false,
            peers: PeerMap::new(),
            topics: TopicMap::new(),
            gossip_buffer: Default::default(),
        }
    }

    /// Runs the sync manager and gossip actor, sets up shutdown handlers and spawns the engine
    /// event loop.
    pub async fn run(
        mut self,
        mut gossip_actor: GossipActor<T>,
        sync_actor: Option<SyncManager<T>>,
    ) -> Result<()> {
        // Used to shutdown the sync manager.
        // @TODO: Instead of introducing a token here would be nice to stick to the `shutdown`
        // method flow as implemented in other actors.
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
        // shutdown completed
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
        let mut announce_topics_interval = interval(ANNOUNCE_TOPICS_INTERVAL);
        let mut join_topics_interval = interval(JOIN_TOPICS_INTERVAL);

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
                // Attempt joining the network-wide gossip if we haven't yet.
                _ = join_network_interval.tick(), if !self.network_joined  => {
                    self.join_topic(self.network_id).await?;
                },
                // Attempt joining the individual topic gossips if we haven't yet.
                _ = join_topics_interval.tick() => {
                    self.join_earmarked_topics().await?;
                },
                // Frequently announce the topics we're interested in to the network-wide gossip.
                _ = announce_topics_interval.tick(), if self.network_joined => {
                    self.announce_topics().await?;
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
            ToEngineActor::NeighborUp { topic_id, peer } => {
                self.on_peer_joined(topic_id, peer).await?;
            }
            ToEngineActor::Received {
                bytes,
                delivered_from,
                topic_id,
            } => {
                self.on_gossip_message(bytes, delivered_from, topic_id)
                    .await?;
            }
            ToEngineActor::SyncHandshakeSuccess { peer, topic_id } => {
                self.gossip_buffer.lock(peer, topic_id);
            }
            ToEngineActor::SyncMessage {
                header,
                payload,
                delivered_from,
                topic_id,
            } => {
                self.on_sync_message(header, payload, delivered_from, topic_id)
                    .await?;
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
            ToEngineActor::TopicJoined { topic_id } => {
                self.on_topic_joined(topic_id).await?;
            }
            ToEngineActor::KnownPeers { reply } => {
                let list = self.peers.known_peers();
                reply.send(Ok(list)).ok();
            }
            ToEngineActor::SyncDone { peer, topic_id } => {
                let counter = self.gossip_buffer.unlock(peer, topic_id);

                if counter == 0 {
                    let buffer = self
                        .gossip_buffer
                        .drain(peer, topic_id)
                        .expect("missing expected gossip buffer");

                    for bytes in buffer {
                        self.on_gossip_message(bytes, peer, topic_id).await?;
                    }
                }
            }
            ToEngineActor::Shutdown { .. } => {
                unreachable!("handled in run_inner");
            }
        }

        Ok(())
    }

    /// Add a peer to our address book.
    ///
    /// Any provided network addresses are registered with the endpoint so that automatic
    /// connection attempts can be made.
    ///
    /// If our node is not currently connected or pending connection to the gossip overlay, attempt
    /// to join.
    async fn add_peer(&mut self, node_addr: NodeAddr) -> Result<()> {
        let node_id = node_addr.node_id;

        // Make sure the low-level networking endpoint also knows about this address, otherwise
        // connection attempts might fail.
        if self.endpoint.add_node_addr(node_addr.clone()).is_err() {
            // This can fail if we're trying to add ourselves.
            debug!("tried to add invalid node {node_id} to known peers list");
            return Ok(());
        }

        if let Some(addr) = self.peers.add_peer(self.network_id, node_addr) {
            debug!(
                "updated address for {} in known peers list: {:?}",
                node_id, addr
            );
        } else {
            debug!("added new peer to handler {}", node_id);

            // Attempt joining network when trying for the first time.
            if !self.network_joined && !self.network_joined_pending {
                self.join_topic(self.network_id).await?;
            }
        }

        Ok(())
    }

    /// Attempt to join the gossip overlay for the given topic if it is of interest to our node.
    ///
    /// The topic may represent the network-wide topic (used for discovering peers and the topics
    /// they're interested in) or it may refer directly to a particular topic of interest.
    async fn join_topic(&mut self, topic_id: [u8; 32]) -> Result<()> {
        if topic_id == self.network_id && !self.network_joined_pending && !self.network_joined {
            self.network_joined_pending = true;
        }

        let peers = self.peers.random_set(&topic_id, JOIN_PEERS_SAMPLE_LEN);
        if !peers.is_empty() {
            self.gossip_actor_tx
                .send(ToGossipActor::Join {
                    topic_id,
                    peers: peers.clone(),
                })
                .await?;
        }

        Ok(())
    }

    /// Update the join status for the given topic, if it is of interest to our node, and announce
    /// all topics.
    async fn on_topic_joined(&mut self, topic_id: [u8; 32]) -> Result<()> {
        if topic_id == self.network_id {
            self.network_joined_pending = false;
            self.network_joined = true;
        }

        self.topics.set_joined(topic_id).await?;
        if topic_id == self.network_id {
            self.announce_topics().await?;
        }

        Ok(())
    }

    /// Register the topic and public key of a peer who just joined the network.
    /// If the topic is of interest to our node, announce all topics.
    async fn on_peer_joined(&mut self, topic_id: [u8; 32], peer_id: PublicKey) -> Result<()> {
        // Add the peer to our address book if they are not already known to us
        if !self.peers.known_peers.contains_key(&peer_id) {
            self.peers.add_peer(topic_id, NodeAddr::new(peer_id));
        }
        if topic_id == self.network_id {
            self.announce_topics().await?;
        }

        Ok(())
    }

    /// Generate a new announcement message for each topic of interest to our node and
    /// broadcast it to the network.
    async fn announce_topics(&mut self) -> Result<()> {
        let topics = self.topics.earmarked().await;
        let message = NetworkMessage::new_announcement(topics);
        let bytes = message.to_bytes();

        self.gossip_actor_tx
            .send(ToGossipActor::Broadcast {
                topic_id: self.network_id,
                bytes,
            })
            .await?;

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
        mut to_network_rx: mpsc::Receiver<ToNetwork>,
        gossip_ready_tx: oneshot::Sender<()>,
    ) -> Result<()> {
        // Keep an earmark that we're interested in joining this topic
        self.topics
            .earmark(topic.clone(), from_network_tx, gossip_ready_tx)
            .await;

        // If we haven't joined a gossip overlay for this topic yet, optimistically try to do it
        // now. If this fails we will retry later in our main loop
        if !self.topics.has_joined(&topic.id()).await {
            self.join_topic(topic.id()).await?;
        }

        // Task to establish a channel for sending messages into gossip overlay
        {
            let gossip_actor_tx = self.gossip_actor_tx.clone();
            let topics = self.topics.clone();
            tokio::task::spawn(async move {
                while let Some(event) = to_network_rx.recv().await {
                    if !topics.has_successfully_joined(&topic.id()).await {
                        // @TODO: We're dropping messages silently for now, later we want to buffer
                        // them somewhere until we've joined the topic gossip
                        continue;
                    }

                    let result = match event {
                        ToNetwork::Message { bytes } => {
                            gossip_actor_tx
                                .send(ToGossipActor::Broadcast {
                                    topic_id: topic.id(),
                                    bytes,
                                })
                                .await
                        }
                    };

                    if let Err(err) = result {
                        error!("failed broadcasting message to gossip for topic {topic:?}: {err}");
                        break;
                    }
                }
            });
        }

        if self.network_joined {
            self.announce_topics().await?;
        }

        Ok(())
    }

    /// Join all earmarked topics which have not yet been successfully joined.
    async fn join_earmarked_topics(&mut self) -> Result<()> {
        for topic_id in self.topics.earmarked().await {
            if !self.topics.has_successfully_joined(&topic_id).await {
                self.join_topic(topic_id).await?;
            }
        }

        Ok(())
    }

    /// Process an message forwarded from the sync actor.
    async fn on_sync_message(
        &mut self,
        header: Vec<u8>,
        payload: Option<Vec<u8>>,
        delivered_from: PublicKey,
        topic_id: [u8; 32],
    ) -> Result<()> {
        if self.topics.has_joined(&topic_id).await {
            self.topics
                .on_sync_message(topic_id, header, payload, delivered_from)
                .await?;
        } else {
            warn!("received message for unknown topic {topic_id:?}");
        }

        Ok(())
    }

    /// Process an inbound message from the network.
    async fn on_gossip_message(
        &mut self,
        bytes: Vec<u8>,
        delivered_from: PublicKey,
        topic_id: [u8; 32],
    ) -> Result<()> {
        if topic_id == self.network_id {
            // Message coming from network-wide gossip overlay
            let Ok(message) = NetworkMessage::from_bytes(&bytes) else {
                warn!(
                    "could not parse network-wide gossip message from {}",
                    delivered_from
                );
                return Ok(());
            };

            // So far we're only expecting one message type on the network-wide overlay
            match message {
                NetworkMessage::Announcement(_, topic_ids) => {
                    self.on_announcement_message(topic_ids, delivered_from)
                        .await?;
                }
            }
        } else if self.topics.has_joined(&topic_id).await {
            if let Some(buffer) = self.gossip_buffer.buffer(delivered_from, topic_id) {
                buffer.push(bytes);
            } else {
                self.topics
                    .on_gossip_message(topic_id, bytes, delivered_from)
                    .await?;
            }
        } else {
            warn!("received message for unknown topic {topic_id:?}");
        }

        Ok(())
    }

    /// Process an announcement message from the gossip overlay.
    async fn on_announcement_message(
        &mut self,
        topic_ids: Vec<[u8; 32]>,
        delivered_from: PublicKey,
    ) -> Result<()> {
        debug!(
            "received announcement of peer {} {:?}",
            delivered_from, topic_ids
        );

        // Register earmarked topics from other peers
        self.peers
            .on_announcement(topic_ids.clone(), delivered_from);

        // And optimistically try to join them if there's an overlap with our interests
        self.join_earmarked_topics().await?;

        // Inform the connection manager about any peer-topic combinations which are of interest to
        // us
        if let Some(sync_actor_tx) = &self.sync_actor_tx {
            let topics_of_interest = self.topics.earmarked().await;
            for topic_id in &topic_ids {
                if topics_of_interest.contains(topic_id) {
                    let topic = self
                        .topics
                        .get(topic_id)
                        .await
                        .expect("expected topic to be present in topic map");
                    let peer_topic = ToSyncActor::new(delivered_from, topic);
                    sync_actor_tx.send(peer_topic).await?
                }
            }
        }

        Ok(())
    }

    /// Deregister our interest in the given topic and leave the gossip overlay.
    #[allow(dead_code)]
    async fn leave_topic(&mut self, topic_id: [u8; 32]) -> Result<()> {
        self.topics.remove_earmark(&topic_id).await;
        self.gossip_actor_tx
            .send(ToGossipActor::Leave { topic_id })
            .await?;
        Ok(())
    }

    /// Shutdown the gossip actor.
    async fn shutdown(&mut self) -> Result<()> {
        self.gossip_actor_tx
            .send(ToGossipActor::Shutdown)
            .await
            .ok();
        Ok(())
    }
}
