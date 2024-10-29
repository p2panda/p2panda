// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use iroh_net::key::PublicKey;
use iroh_net::{Endpoint, NodeAddr, NodeId};
use p2panda_sync::Topic;
use rand::seq::IteratorRandom;
use tokio::sync::{broadcast, mpsc, oneshot, RwLock};
use tokio::time::interval;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, warn};

use crate::engine::gossip::{GossipActor, ToGossipActor};
use crate::engine::message::NetworkMessage;
use crate::network::{FromNetwork, ToNetwork};
use crate::sync::manager::{SyncManager, ToSyncManager};
use crate::{FromBytes, ToBytes, TopicId};

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

#[derive(Debug, Default)]
pub struct GossipBuffer {
    buffers: HashMap<(PublicKey, [u8; 32]), Vec<Vec<u8>>>,
    counters: HashMap<(PublicKey, [u8; 32]), usize>,
}

impl GossipBuffer {
    fn lock(&mut self, peer: PublicKey, topic_id: [u8; 32]) {
        let counter = self.counters.entry((peer, topic_id)).or_default();
        *counter += 1;

        self.buffers.entry((peer, topic_id)).or_default();

        // @TODO: bring back assertion for checking we have max 2 concurrent sync sessions per peer+topic_id
        debug!(
            "lock gossip buffer with {} on topic {:?}: {}",
            peer, topic_id, counter
        );
    }

    fn unlock(&mut self, peer: PublicKey, topic_id: [u8; 32]) -> usize {
        match self.counters.get_mut(&(peer, topic_id)) {
            Some(counter) => {
                *counter -= 1;
                debug!(
                    "unlock gossip buffer with {} on topic {:?}: {}",
                    peer, topic_id, counter
                );
                *counter
            }
            None => panic!(),
        }
    }

    fn drain(&mut self, peer: PublicKey, topic_id: [u8; 32]) -> Option<Vec<Vec<u8>>> {
        self.buffers.remove(&(peer, topic_id))
    }

    fn buffer(&mut self, peer: PublicKey, topic_id: [u8; 32]) -> Option<&mut Vec<Vec<u8>>> {
        self.buffers.get_mut(&(peer, topic_id))
    }
}

pub struct EngineActor<T> {
    endpoint: Endpoint,
    gossip_actor_tx: mpsc::Sender<ToGossipActor>,
    sync_manager_tx: Option<mpsc::Sender<ToSyncManager<T>>>,
    inbox: mpsc::Receiver<ToEngineActor<T>>,
    // @TODO: Think about field naming here; perhaps these fields would be more accurately prefixed
    // by `topic_` or `gossip_`, since they are not referencing the overall network swarm (aka.
    // network-wide gossip overlay).
    network_id: [u8; 32],
    network_joined: bool,
    network_joined_pending: bool,
    peers: PeerMap,
    topics: TopicMap<T>,
    gossip_buffer: GossipBuffer,
}

impl<T> EngineActor<T>
where
    T: Topic + TopicId + 'static,
{
    pub fn new(
        endpoint: Endpoint,
        inbox: mpsc::Receiver<ToEngineActor<T>>,
        gossip_actor_tx: mpsc::Sender<ToGossipActor>,
        sync_manager_tx: Option<mpsc::Sender<ToSyncManager<T>>>,
        network_id: [u8; 32],
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
        mut gossip_actor: GossipActor<T>,
        sync_manager: Option<SyncManager<T>>,
    ) -> Result<()> {
        // Used to shutdown the sync manager.
        let shutdown_token = CancellationToken::new();
        let cloned_shutdown_token = shutdown_token.clone();

        if let Some(sync_manager) = sync_manager {
            tokio::task::spawn(async move {
                if let Err(err) = sync_manager.run(cloned_shutdown_token).await {
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
            ToEngineActor::Shutdown { .. } => {
                unreachable!("handled in run_inner");
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

            // Do not attempt peer sync if the topic_id is the network id.
            if topic_id == self.network_id {
                return Ok(());
            }
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
        if let Some(sync_manager_tx) = &self.sync_manager_tx {
            let topics_of_interest = self.topics.earmarked().await;
            for topic_id in &topic_ids {
                if topics_of_interest.contains(topic_id) {
                    let topic = self
                        .topics
                        .get(topic_id)
                        .await
                        .expect("expected topic to be present in topic map");
                    let peer_topic = ToSyncManager::new(delivered_from, topic);
                    sync_manager_tx.send(peer_topic).await?
                }
            }
        }

        Ok(())
    }

    #[allow(dead_code)]
    /// Deregister our interest in the given topic and leave the gossip overlay.
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

#[derive(Clone, Debug)]
struct TopicMap<T> {
    inner: Arc<RwLock<TopicMapInner<T>>>,
}

/// The topic associated with a particular subscription along with it's broadcast channel and
/// oneshot ready channel.
type TopicMeta<T> = (
    T,
    broadcast::Sender<FromNetwork>,
    Option<oneshot::Sender<()>>,
);

#[derive(Debug)]
struct TopicMapInner<T> {
    earmarked: HashMap<[u8; 32], TopicMeta<T>>,
    pending_joins: HashSet<[u8; 32]>,
    joined: HashSet<[u8; 32]>,
}

impl<T> TopicMap<T>
where
    T: Topic + TopicId,
{
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

    pub async fn get(&self, topic_id: &[u8; 32]) -> Option<T> {
        let inner = self.inner.read().await;
        inner
            .earmarked
            .get(topic_id)
            .map(|(topic, _, _)| topic.clone())
    }

    /// Mark a topic of interest to our node.
    pub async fn earmark(
        &mut self,
        topic: T,
        from_network_tx: broadcast::Sender<FromNetwork>,
        gossip_ready_tx: oneshot::Sender<()>,
    ) {
        let mut inner = self.inner.write().await;
        inner.earmarked.insert(
            topic.id(),
            (topic.clone(), from_network_tx, Some(gossip_ready_tx)),
        );
        inner.pending_joins.insert(topic.id());
    }

    /// Remove a topic of interest to our node.
    pub async fn remove_earmark(&mut self, topic_id: &[u8; 32]) {
        let mut inner = self.inner.write().await;
        inner.earmarked.remove(topic_id);
        inner.pending_joins.remove(topic_id);
    }

    /// Return a list of topics of interest to our node.
    pub async fn earmarked(&self) -> Vec<[u8; 32]> {
        let inner = self.inner.read().await;
        inner.earmarked.keys().cloned().collect()
    }

    /// Mark that we've successfully joined a gossip overlay for this topic.
    pub async fn set_joined(&mut self, topic_id: [u8; 32]) -> Result<()> {
        let mut inner = self.inner.write().await;
        if inner.pending_joins.remove(&topic_id) {
            inner.joined.insert(topic_id);

            // Inform local topic subscribers that the gossip overlay has been joined and is ready
            // for messages.
            if let Some((_, _, gossip_ready_tx)) = inner.earmarked.get_mut(&topic_id) {
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
    pub async fn has_successfully_joined(&self, topic_id: &[u8; 32]) -> bool {
        let inner = self.inner.read().await;
        inner.joined.contains(topic_id)
    }

    /// Return true if there's either a pending or successfully joined gossip overlay for this
    /// topic.
    pub async fn has_joined(&self, topic_id: &[u8; 32]) -> bool {
        let inner = self.inner.read().await;
        inner.joined.contains(topic_id) || inner.pending_joins.contains(topic_id)
    }

    /// Handle incoming messages from gossip.
    ///
    /// This method forwards messages to the subscribers for the given topic.
    pub async fn on_gossip_message(
        &self,
        topic_id: [u8; 32],
        bytes: Vec<u8>,
        delivered_from: PublicKey,
    ) -> Result<()> {
        let inner = self.inner.read().await;
        let (_, from_network_tx, _gossip_ready_tx) = inner
            .earmarked
            .get(&topic_id)
            .context("on_gossip_message")?;
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
        topic_id: [u8; 32],
        header: Vec<u8>,
        payload: Option<Vec<u8>>,
        delivered_from: PublicKey,
    ) -> Result<()> {
        let inner = self.inner.read().await;
        let (_, from_network_tx, _) = inner.earmarked.get(&topic_id).context("on_sync_message")?;
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
    topics: HashMap<[u8; 32], Vec<PublicKey>>,
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
    pub fn add_peer(&mut self, topic_id: [u8; 32], node_addr: NodeAddr) -> Option<NodeAddr> {
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
            self.on_announcement(vec![topic_id], public_key);
            self.known_peers.insert(public_key, node_addr)
        }
    }

    /// Update the topics our node knows about, including the public key of the peer who announced
    /// the topic.
    pub fn on_announcement(&mut self, topics: Vec<[u8; 32]>, delivered_from: PublicKey) {
        for topic_id in topics {
            match self.topics.get_mut(&topic_id) {
                Some(list) => {
                    if !list.contains(&delivered_from) {
                        list.push(delivered_from)
                    }
                }
                None => {
                    self.topics.insert(topic_id, vec![delivered_from]);
                }
            }
        }
    }

    /// Return a random set of known peers with an interest in the given topic.
    pub fn random_set(&self, topic_id: &[u8; 32], size: usize) -> Vec<NodeId> {
        self.topics
            .get(topic_id)
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
