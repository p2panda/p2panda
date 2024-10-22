// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use iroh_gossip::proto::TopicId;
use iroh_net::key::PublicKey;
use iroh_net::{Endpoint, NodeAddr, NodeId};
use rand::seq::IteratorRandom;
use tokio::sync::{broadcast, mpsc, oneshot, RwLock};
use tokio::time::interval;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, warn};

use crate::engine::gossip::{GossipActor, ToGossipActor};
use crate::engine::message::NetworkMessage;
use crate::network::{FromNetwork, ToNetwork};
use crate::sync::manager::{SyncManager, ToSyncManager};
use crate::{FromBytes, ToBytes};

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
        from_network_tx: broadcast::Sender<FromNetwork>,
        to_network_rx: mpsc::Receiver<ToNetwork>,
        gossip_ready_tx: oneshot::Sender<()>,
    },
    Received {
        bytes: Vec<u8>,
        delivered_from: PublicKey,
        topic: TopicId,
    },
    SyncHandshakeSuccess {
        peer: PublicKey,
        topic: TopicId,
    },
    SyncMessage {
        header: Vec<u8>,
        payload: Option<Vec<u8>>,
        delivered_from: PublicKey,
        topic: TopicId,
    },
    SyncDone {
        peer: PublicKey,
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

#[derive(Debug, Default)]
pub struct GossipBuffer {
    buffers: HashMap<(PublicKey, TopicId), Vec<Vec<u8>>>,
    counters: HashMap<(PublicKey, TopicId), usize>,
}

impl GossipBuffer {
    fn lock(&mut self, peer: PublicKey, topic: TopicId) {
        let counter = self.counters.entry((peer, topic)).or_default();
        *counter += 1;

        self.buffers.entry((peer, topic)).or_default();

        // @TODO: bring back assertion for checking we have max 2 concurrent sync sessions per peer+topic
        debug!(
            "lock gossip buffer with {} on topic {}: {}",
            peer, topic, counter
        );
    }

    fn unlock(&mut self, peer: PublicKey, topic: TopicId) -> usize {
        match self.counters.get_mut(&(peer, topic)) {
            Some(counter) => {
                *counter -= 1;
                debug!(
                    "unlock gossip buffer with {} on topic {}: {}",
                    peer, topic, counter
                );
                *counter
            }
            None => panic!(),
        }
    }

    fn drain(&mut self, peer: PublicKey, topic: TopicId) -> Option<Vec<Vec<u8>>> {
        self.buffers.remove(&(peer, topic))
    }

    fn buffer(&mut self, peer: PublicKey, topic: TopicId) -> Option<&mut Vec<Vec<u8>>> {
        self.buffers.get_mut(&(peer, topic))
    }
}

pub struct EngineActor {
    endpoint: Endpoint,
    gossip_actor_tx: mpsc::Sender<ToGossipActor>,
    sync_manager_tx: Option<mpsc::Sender<ToSyncManager>>,
    inbox: mpsc::Receiver<ToEngineActor>,
    // @TODO: Think about field naming here; perhaps these fields would be more accurately prefixed
    // by `topic_` or `gossip_`, since they are not referencing the overall network swarm (aka.
    // network-wide gossip overlay).
    network_id: TopicId,
    network_joined: bool,
    network_joined_pending: bool,
    peers: PeerMap,
    topics: TopicMap,
    gossip_buffer: GossipBuffer,
}

impl EngineActor {
    pub fn new(
        endpoint: Endpoint,
        inbox: mpsc::Receiver<ToEngineActor>,
        gossip_actor_tx: mpsc::Sender<ToGossipActor>,
        sync_manager_tx: Option<mpsc::Sender<ToSyncManager>>,
        network_id: TopicId,
    ) -> Self {
        Self {
            endpoint,
            gossip_actor_tx,
            sync_manager_tx,
            inbox,
            network_id,
            network_joined: false,
            network_joined_pending: false,
            peers: PeerMap::new(),
            topics: TopicMap::new(),
            gossip_buffer: Default::default(),
        }
    }

    pub async fn run(
        mut self,
        mut gossip_actor: GossipActor,
        sync_manager: Option<SyncManager>,
    ) -> Result<()> {
        let sync_manager_token = CancellationToken::new();
        let cloned_sync_manager_token = sync_manager_token.clone();

        if let Some(sync_manager) = sync_manager {
            tokio::task::spawn(async move {
                if let Err(err) = sync_manager.run(cloned_sync_manager_token).await {
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

        sync_manager_token.cancel();
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
            ToEngineActor::SyncHandshakeSuccess { peer, topic } => {
                self.gossip_buffer.lock(peer, topic);
            }
            ToEngineActor::SyncMessage {
                header,
                payload,
                delivered_from,
                topic,
            } => {
                self.on_sync_message(header, payload, delivered_from, topic)
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
            ToEngineActor::SyncDone { peer, topic } => {
                let counter = self.gossip_buffer.unlock(peer, topic);

                if counter == 0 {
                    let buffer = self
                        .gossip_buffer
                        .drain(peer, topic)
                        .expect("missing expected gossip buffer");

                    for bytes in buffer {
                        self.on_gossip_message(bytes, peer, topic).await?;
                    }
                }
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
                .send(ToGossipActor::Join {
                    topic,
                    peers: peers.clone(),
                })
                .await?;

            // Do not attempt peer sync if the topic is the network id.
            if topic == self.network_id {
                return Ok(());
            }
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

        self.gossip_actor_tx
            .send(ToGossipActor::Broadcast {
                topic: self.network_id,
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
        topic: TopicId,
        from_network_tx: broadcast::Sender<FromNetwork>,
        mut to_network_rx: mpsc::Receiver<ToNetwork>,
        gossip_ready_tx: oneshot::Sender<()>,
    ) -> Result<()> {
        // Keep an earmark that we're interested in joining this topic
        self.topics
            .earmark(topic, from_network_tx, gossip_ready_tx)
            .await;

        // If we haven't joined a gossip overlay for this topic yet, optimistically try to do it
        // now. If this fails we will retry later in our main loop
        if !self.topics.has_joined(&topic).await {
            self.join_topic(topic).await?;
        }

        // Task to establish a channel for sending messages into gossip overlay
        {
            let gossip_actor_tx = self.gossip_actor_tx.clone();
            let topics = self.topics.clone();
            tokio::task::spawn(async move {
                while let Some(event) = to_network_rx.recv().await {
                    if !topics.has_successfully_joined(&topic).await {
                        // @TODO: We're dropping messages silently for now, later we want to buffer
                        // them somewhere until we've joined the topic gossip
                        continue;
                    }

                    let result = match event {
                        ToNetwork::Message { bytes } => {
                            gossip_actor_tx
                                .send(ToGossipActor::Broadcast { topic, bytes })
                                .await
                        }
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

    /// Process an message forwarded from the sync actor.
    async fn on_sync_message(
        &mut self,
        header: Vec<u8>,
        payload: Option<Vec<u8>>,
        delivered_from: PublicKey,
        topic: TopicId,
    ) -> Result<()> {
        if self.topics.has_joined(&topic).await {
            self.topics
                .on_sync_message(topic, header, payload, delivered_from)
                .await?;
        } else {
            warn!("received message for unknown topic {topic}");
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
            if let Some(buffer) = self.gossip_buffer.buffer(delivered_from, topic) {
                buffer.push(bytes);
            } else {
                self.topics
                    .on_gossip_message(topic, bytes, delivered_from)
                    .await?;
            }
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
        self.peers.on_announcement(topics.clone(), delivered_from);

        // And optimistically try to join them if there's an overlap with our interests
        self.join_earmarked_topics().await?;

        // Inform the connection manager about the peer topics
        //
        // NOTE: This will only return once sync has been attempted with all novel peer-topic
        // combinations.
        if let Some(sync_manager_tx) = &self.sync_manager_tx {
            //sync_manager
            //    .update_peer_topics(delivered_from, topics)
            //    .await?;

            let peer_topics = ToSyncManager::new(delivered_from, topics);
            sync_manager_tx.send(peer_topics).await?
        }

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
    earmarked: HashMap<TopicId, (broadcast::Sender<FromNetwork>, Option<oneshot::Sender<()>>)>,
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
    pub async fn earmark(
        &mut self,
        topic: TopicId,
        from_network_tx: broadcast::Sender<FromNetwork>,
        gossip_ready_tx: oneshot::Sender<()>,
    ) {
        let mut inner = self.inner.write().await;
        inner
            .earmarked
            .insert(topic, (from_network_tx, Some(gossip_ready_tx)));
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

            // Inform local topic subscribers that the gossip overlay has been joined and is ready
            // for messages.
            if let Some((_from_network_tx, gossip_ready_tx)) = inner.earmarked.get_mut(&topic) {
                // We need the `Sender` to be owned so we take it and replace with `None`.
                if let Some(oneshot_tx) = gossip_ready_tx.take() {
                    if oneshot_tx.send(()).is_err() {
                        warn!("gossip topic oneshot ready receiver dropped")
                    }
                }
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

    /// Handle incoming messages from gossip.
    ///
    /// This method forwards messages to the subscribers for the given topic.
    pub async fn on_gossip_message(
        &self,
        topic: TopicId,
        bytes: Vec<u8>,
        delivered_from: PublicKey,
    ) -> Result<()> {
        let inner = self.inner.read().await;
        let (from_network_tx, _gossip_ready_tx) =
            inner.earmarked.get(&topic).context("on_gossip_message")?;
        from_network_tx.send(FromNetwork::GossipMessage {
            bytes,
            delivered_from: to_public_key(delivered_from),
        })?;
        Ok(())
    }

    /// Handle incoming messages from sync.
    ///
    /// This method forwards messages to the subscribers for the given topic.
    pub async fn on_sync_message(
        &self,
        topic: TopicId,
        header: Vec<u8>,
        payload: Option<Vec<u8>>,
        delivered_from: PublicKey,
    ) -> Result<()> {
        let inner = self.inner.read().await;
        let (from_network_tx, _) = inner.earmarked.get(&topic).context("on_sync_message")?;
        from_network_tx.send(FromNetwork::SyncMessage {
            header,
            payload,
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
