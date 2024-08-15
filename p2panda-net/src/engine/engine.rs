// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use iroh_gossip::net::Gossip;
use iroh_gossip::proto::TopicId;
use iroh_net::key::PublicKey;
use iroh_net::{Endpoint, NodeAddr, NodeId};
use rand::seq::IteratorRandom;
use tokio::sync::{broadcast, mpsc, oneshot, RwLock};
use tokio::time::interval;
use tracing::{debug, error, warn};

use crate::engine::gossip::{GossipActor, ToGossipActor};
use crate::engine::message::NetworkMessage;
use crate::message::{FromBytes, ToBytes};
use crate::network::{InEvent, OutEvent};

/// Maximum size of random sample set when choosing peers to join gossip overlay.
///
/// The larger the number the less likely joining the gossip will fail as we get more chances to
/// establish connections. As soon as we've joined the gossip we will learn about more peers.
const JOIN_PEERS_SAMPLE_LEN: usize = 7;

/// In what frequency do we attempt joining the network-wide gossip overlay over a newly, randomly
/// sampled set of peers.
const JOIN_NETWORK_INTERVAL: Duration = Duration::from_millis(900);

/// How often do we announce the list of our subscribed topics.
const ANNOUNCE_TOPICS_INTERVAL: Duration = Duration::from_millis(2200);

/// How often do we try to join the topics we're interested in.
const JOIN_TOPICS_INTERVAL: Duration = Duration::from_millis(1200);

pub enum ToEngineActor {
    AddPeer {
        node_addr: NodeAddr,
    },
    NeighborUp {
        topic: TopicId,
        peer: PublicKey,
    },
    Subscribe {
        topic: TopicId,
        out_tx: broadcast::Sender<OutEvent>,
        in_rx: mpsc::Receiver<InEvent>,
    },
    Received {
        bytes: Vec<u8>,
        delivered_from: PublicKey,
        topic: TopicId,
    },
    Shutdown {
        reply: oneshot::Sender<()>,
    },
    TopicJoined {
        topic: TopicId,
    },
    KnownPeers {
        reply: oneshot::Sender<Result<Vec<NodeAddr>>>,
    },
}

pub struct EngineActor {
    endpoint: Endpoint,
    gossip: Gossip,
    gossip_actor_tx: mpsc::Sender<ToGossipActor>,
    inbox: mpsc::Receiver<ToEngineActor>,
    // @TODO: Think about field naming here; perhaps these fields would be more accurately prefixed
    // by `topic_` or `gossip_`, since they are not referencing the overall network swarm (aka.
    // network-wide gossip overlay).
    network_id: TopicId,
    network_joined: bool,
    network_joined_pending: bool,
    peers: PeerMap,
    topics: TopicMap,
}

impl EngineActor {
    pub fn new(
        endpoint: Endpoint,
        gossip: Gossip,
        gossip_actor_tx: mpsc::Sender<ToGossipActor>,
        inbox: mpsc::Receiver<ToEngineActor>,
        network_id: TopicId,
    ) -> Self {
        Self {
            endpoint,
            gossip,
            gossip_actor_tx,
            inbox,
            network_id,
            network_joined: false,
            network_joined_pending: false,
            peers: PeerMap::new(),
            topics: TopicMap::new(),
        }
    }

    pub async fn run(mut self, mut gossip_actor: GossipActor) -> Result<()> {
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
                // Attempt joining the network-wide gossip if we haven't yet
                _ = join_network_interval.tick(), if !self.network_joined  => {
                    self.join_topic(self.network_id).await?;
                },
                // Attempt joining the individual topic gossips if we haven't yet
                _ = join_topics_interval.tick() => {
                    self.join_earmarked_topics().await?;
                },
                // Frequently announce the topics we're interested in in the network-wide gossip
                _ = announce_topics_interval.tick(), if self.network_joined => {
                    self.announce_topics().await?;
                },
            }
        }
    }

    async fn on_actor_message(&mut self, msg: ToEngineActor) -> Result<()> {
        match msg {
            ToEngineActor::AddPeer { node_addr } => {
                self.add_peer(node_addr).await?;
            }
            ToEngineActor::NeighborUp { topic, peer } => {
                self.on_peer_joined(topic, peer).await?;
            }
            ToEngineActor::Received {
                bytes,
                delivered_from,
                topic,
            } => {
                self.on_gossip_message(bytes, delivered_from, topic).await?;
            }
            ToEngineActor::Subscribe {
                topic,
                out_tx,
                in_rx,
            } => {
                self.on_subscribe(topic, out_tx, in_rx).await?;
            }
            ToEngineActor::TopicJoined { topic } => {
                self.on_topic_joined(topic).await?;
            }
            ToEngineActor::KnownPeers { reply } => {
                let list = self.peers.known_peers();
                reply.send(Ok(list)).ok();
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

        // Make sure the endpoint also knows about this address
        match self.endpoint.add_node_addr(node_addr.clone()) {
            Ok(_) => {
                if let Some(addr) = self.peers.add_peer(self.network_id, node_addr) {
                    debug!(
                        "updated address for {} in known peers list: {:?}",
                        node_id, addr
                    );
                } else {
                    debug!("added new peer to handler {}", node_id);

                    // Attempt joining network when trying for the first time
                    if !self.network_joined && !self.network_joined_pending {
                        self.join_topic(self.network_id).await?;
                    }
                }
            }
            Err(err) => {
                // This can fail if we're trying to add ourselves
                debug!(
                    "tried to add invalid node {} to known peers list: {err}",
                    node_id
                );
            }
        }

        Ok(())
    }

    // @TODO: Need to be sure that comments correctly differentiate between the network-wide gossip
    // overlay (swarm) and the individual gossip overlays for each topic.
    /// Attempt to join the gossip overlay for the given topic if it is of interest to our node.
    async fn join_topic(&mut self, topic: TopicId) -> Result<()> {
        if topic == self.network_id && !self.network_joined_pending && !self.network_joined {
            self.network_joined_pending = true;
        }

        let peers = self.peers.random_set(&topic, JOIN_PEERS_SAMPLE_LEN);
        if !peers.is_empty() {
            self.gossip_actor_tx
                .send(ToGossipActor::Join { topic, peers })
                .await?;
        }

        Ok(())
    }

    /// Update the join status for the given topic, if it is of interest to our node, and announce
    /// all topics.
    async fn on_topic_joined(&mut self, topic: TopicId) -> Result<()> {
        if topic == self.network_id {
            self.network_joined_pending = false;
            self.network_joined = true;
        }

        self.topics.set_joined(topic).await?;
        if topic == self.network_id {
            self.announce_topics().await?;
        }

        Ok(())
    }

    /// Register the topic and public key of a peer who just joined the network.
    /// If the topic is of interest to our node, announce all topics.
    async fn on_peer_joined(&mut self, topic: TopicId, peer_id: PublicKey) -> Result<()> {
        // Add the peer to our address book if they are not already known to us
        if !self.peers.known_peers.contains_key(&peer_id) {
            self.peers.add_peer(topic, NodeAddr::new(peer_id));
        }
        if topic == self.network_id {
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
        self.gossip.broadcast(self.network_id, bytes.into()).await?;
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
        topic: TopicId,
        out_tx: broadcast::Sender<OutEvent>,
        mut in_rx: mpsc::Receiver<InEvent>,
    ) -> Result<()> {
        // Keep an earmark that we're interested in joining this topic
        self.topics.earmark(topic, out_tx).await;

        // If we haven't joined a gossip overlay for this topic yet, optimistically try to do it
        // now. If this fails we will retry later in our main loop
        if !self.topics.has_joined(&topic).await {
            self.join_topic(topic).await?;
        }

        // Task to establish a channel for sending messages into gossip overlay
        {
            let gossip = self.gossip.clone();
            let topics = self.topics.clone();
            tokio::task::spawn(async move {
                while let Some(event) = in_rx.recv().await {
                    if !topics.has_successfully_joined(&topic).await {
                        // @TODO: We're dropping messages silently for now, later we want to buffer
                        // them somewhere until we've joined the topic gossip
                        continue;
                    }

                    let result = match event {
                        InEvent::Message { bytes } => gossip.broadcast(topic, bytes.into()).await,
                    };

                    if let Err(err) = result {
                        error!("failed broadcasting message to gossip for topic {topic}: {err}");
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
        for topic in self.topics.earmarked().await {
            if !self.topics.has_successfully_joined(&topic).await {
                self.join_topic(topic).await?;
            }
        }

        Ok(())
    }

    /// Process an inbound message from the network.
    async fn on_gossip_message(
        &mut self,
        bytes: Vec<u8>,
        delivered_from: PublicKey,
        topic: TopicId,
    ) -> Result<()> {
        if topic == self.network_id {
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
                NetworkMessage::Announcement(_, topics) => {
                    self.on_announcement_message(topics, delivered_from).await?;
                }
            }
        } else if self.topics.has_joined(&topic).await {
            self.topics.on_message(topic, bytes, delivered_from).await?;
        } else {
            warn!("received message for unknown topic {topic}");
        }

        Ok(())
    }

    /// Process an announcement message from the gossip overlay.
    async fn on_announcement_message(
        &mut self,
        topics: Vec<TopicId>,
        delivered_from: PublicKey,
    ) -> Result<()> {
        debug!(
            "received announcement of peer {} {:?}",
            delivered_from, topics
        );

        // Register earmarked topics from other peers
        self.peers.on_announcement(topics, delivered_from);

        // And optimistically try to join them if there's an overlap with our interests
        self.join_earmarked_topics().await?;

        Ok(())
    }

    #[allow(dead_code)]
    /// Deregister our interest in the given topic and leave the gossip overlay.
    async fn leave_topic(&mut self, topic: TopicId) -> Result<()> {
        self.topics.remove_earmark(&topic).await;
        self.gossip_actor_tx
            .send(ToGossipActor::Leave { topic })
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

#[derive(Clone, Debug)]
struct TopicMap {
    inner: Arc<RwLock<TopicMapInner>>,
}

#[derive(Debug)]
struct TopicMapInner {
    earmarked: HashMap<TopicId, broadcast::Sender<OutEvent>>,
    pending_joins: HashSet<TopicId>,
    joined: HashSet<TopicId>,
}

impl TopicMap {
    /// Generate an empty topic map.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(TopicMapInner {
                earmarked: HashMap::new(),
                pending_joins: HashSet::new(),
                joined: HashSet::new(),
            })),
        }
    }

    /// Mark a topic of interest to our node.
    pub async fn earmark(&mut self, topic: TopicId, out_tx: broadcast::Sender<OutEvent>) {
        let mut inner = self.inner.write().await;
        inner.earmarked.insert(topic, out_tx);
        inner.pending_joins.insert(topic);
    }

    /// Remove a topic of interest to our node.
    pub async fn remove_earmark(&mut self, topic: &TopicId) {
        let mut inner = self.inner.write().await;
        inner.earmarked.remove(topic);
        inner.pending_joins.remove(topic);
    }

    /// Return a list of topics of interest to our node.
    pub async fn earmarked(&self) -> Vec<TopicId> {
        let inner = self.inner.read().await;
        inner.earmarked.keys().cloned().collect()
    }

    /// Mark that we've successfully joined a gossip overlay for this topic.
    pub async fn set_joined(&mut self, topic: TopicId) -> Result<()> {
        let mut inner = self.inner.write().await;
        if inner.pending_joins.remove(&topic) {
            inner.joined.insert(topic);

            // If any subscribers, inform them that this channel is ready for messages now
            if let Some(out_tx) = inner.earmarked.get(&topic) {
                out_tx.send(OutEvent::Ready)?;
            }
        }

        Ok(())
    }

    /// Return true if we've successfully joined a gossip overlay for this topic.
    pub async fn has_successfully_joined(&self, topic: &TopicId) -> bool {
        let inner = self.inner.read().await;
        inner.joined.contains(topic)
    }

    /// Return true if there's either a pending or successfully joined gossip overlay for this
    /// topic.
    pub async fn has_joined(&self, topic: &TopicId) -> bool {
        let inner = self.inner.read().await;
        inner.joined.contains(topic) || inner.pending_joins.contains(topic)
    }

    /// Handle incoming messages from gossip overlay.
    ///
    /// This method forwards messages to the subscribers for the given topic.
    pub async fn on_message(
        &self,
        topic: TopicId,
        bytes: Vec<u8>,
        delivered_from: PublicKey,
    ) -> Result<()> {
        let inner = self.inner.read().await;
        let out_tx = inner.earmarked.get(&topic).context("on_message")?;
        out_tx.send(OutEvent::Message {
            bytes,
            delivered_from: to_public_key(delivered_from),
        })?;
        Ok(())
    }
}

struct PeerMap {
    known_peers: HashMap<NodeId, NodeAddr>,
    topics: HashMap<TopicId, Vec<PublicKey>>,
}

impl PeerMap {
    /// Generate an empty peer map.
    pub fn new() -> Self {
        Self {
            known_peers: HashMap::new(),
            topics: HashMap::new(),
        }
    }

    /// Return the public key and addresses for all peers known to our node.
    pub fn known_peers(&self) -> Vec<NodeAddr> {
        self.known_peers.values().cloned().collect()
    }

    /// Update our peer address book.
    ///
    /// If the peer is already known, their node addresses and relay URL are updated.
    /// If not, the peer and their addresses are added to the address book and the local topic
    /// updater is called.
    pub fn add_peer(&mut self, topic: TopicId, node_addr: NodeAddr) -> Option<NodeAddr> {
        let public_key = node_addr.node_id;

        // If the given peer is already known to us, only update the direct addresses and relay url
        // if the supplied values are not empty. This avoids overwriting values with blanks.
        if let Some(addr) = self.known_peers.get_mut(&public_key) {
            if !node_addr.info.is_empty() {
                addr.info
                    .direct_addresses
                    .clone_from(&node_addr.info.direct_addresses);
            }
            if node_addr.relay_url().is_some() {
                addr.info.relay_url = node_addr.info.relay_url;
            }
            Some(addr.clone())
        } else {
            self.on_announcement(vec![topic], public_key);
            self.known_peers.insert(public_key, node_addr)
        }
    }

    /// Update the topics our node knows about, including the public key of the peer who announced
    /// the topic.
    pub fn on_announcement(&mut self, topics: Vec<TopicId>, delivered_from: PublicKey) {
        for topic in topics {
            match self.topics.get_mut(&topic) {
                Some(list) => {
                    if !list.contains(&delivered_from) {
                        list.push(delivered_from)
                    }
                }
                None => {
                    self.topics.insert(topic, vec![delivered_from]);
                }
            }
        }
    }

    /// Return a random set of known peers with an interest in the given topic.
    pub fn random_set(&self, topic: &TopicId, size: usize) -> Vec<NodeId> {
        self.topics
            .get(topic)
            .unwrap_or(&vec![])
            .iter()
            .choose_multiple(&mut rand::thread_rng(), size)
            .into_iter()
            .cloned()
            .collect()
    }
}

fn to_public_key(key: PublicKey) -> p2panda_core::PublicKey {
    p2panda_core::PublicKey::from_bytes(key.as_bytes()).expect("already validated public key")
}
