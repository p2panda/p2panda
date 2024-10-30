// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::Result;
use iroh_net::NodeId;
use p2panda_sync::Topic;
use tokio::sync::{broadcast, mpsc, oneshot, RwLock};
use tracing::{debug, error, warn};

use crate::engine::address_book::AddressBook;
use crate::engine::constants::JOIN_PEERS_SAMPLE_LEN;
use crate::engine::gossip::ToGossipActor;
use crate::engine::gossip_buffer::GossipBuffer;
use crate::network::{FromNetwork, ToNetwork};
use crate::sync::manager::ToSyncActor;
use crate::{to_public_key, TopicId};

/// Managed data stream over an application-defined topic.
type TopicStream<T> = (T, broadcast::Sender<FromNetwork>);

/// Every stream has a unique identifier.
type TopicStreamId = usize;

/// Manages subscriptions to topics in form of data streams.
///
/// A stream has quite a bit of state to deal with, this includes:
/// 1. Try to enter a gossip overlay for sending messages in "live mode" over a topic id.
/// 2. Help the sync manager with learning about topics of interest and guide it to connect to
///    peers for syncing up state with them.
/// 3. Intercept and temporarily buffer incoming gossip messages of a peer when we're currently in
///    a sync session with them. As soon as this sync session has finished we can re-play the
///    messages. This helps reducing the number of out-of-order messages.
/// 4. Applications can subscribe to topics multiple times, or to different topics but with the
///    same topic ids. This stream handler multiplexes messages to the right place, even when
///    there's duplicates.
#[derive(Debug)]
pub struct TopicStreams<T> {
    address_book: AddressBook,
    gossip_actor_tx: mpsc::Sender<ToGossipActor>,
    gossip_buffer: GossipBuffer,
    gossip_joined: Arc<RwLock<HashSet<[u8; 32]>>>,
    gossip_pending: HashMap<[u8; 32], oneshot::Sender<()>>,
    next_stream_id: usize,
    subscribed: HashMap<TopicStreamId, TopicStream<T>>,
    topic_id_to_stream: HashMap<[u8; 32], Vec<TopicStreamId>>,
    topic_to_stream: HashMap<T, Vec<TopicStreamId>>,
    sync_actor_tx: Option<mpsc::Sender<ToSyncActor<T>>>,
}

impl<T> TopicStreams<T>
where
    T: Topic + TopicId + 'static,
{
    pub fn new(
        gossip_actor_tx: mpsc::Sender<ToGossipActor>,
        address_book: AddressBook,
        sync_actor_tx: Option<mpsc::Sender<ToSyncActor<T>>>,
    ) -> Self {
        Self {
            address_book,
            gossip_actor_tx,
            gossip_buffer: Default::default(),
            gossip_joined: Arc::new(RwLock::new(HashSet::new())),
            gossip_pending: HashMap::new(),
            next_stream_id: 1,
            subscribed: HashMap::new(),
            topic_id_to_stream: HashMap::new(),
            topic_to_stream: HashMap::new(),
            sync_actor_tx,
        }
    }

    /// Establishes a stream to send to and receive from an application-defined topic in the
    /// network.
    ///
    /// Internally this already attempts joining the gossip overlay for the topic id to allow "live
    /// mode". At the same time it prepares all data types to be able to manage sync sessions over
    /// the given topic.
    ///
    /// Users can subscribe multiple times to the same topic or to different topics which hold the
    /// same topic ids. The code internally multiplexes duplicate subscriptions and routes messages
    /// to all relevant handlers.
    pub async fn subscribe(
        &mut self,
        topic: T,
        from_network_tx: broadcast::Sender<FromNetwork>,
        mut to_network_rx: mpsc::Receiver<ToNetwork>,
        gossip_ready_tx: oneshot::Sender<()>,
    ) -> Result<()> {
        // Every subscription stream receives its own unique identifier.
        let stream_id = self.next_stream_id;
        self.next_stream_id += 1;

        // Prepare all relevant earmarks and data streams to aid other processes dealing with
        // gossip, buffering or sync.
        self.subscribed
            .insert(stream_id, (topic.clone(), from_network_tx));
        self.gossip_pending.insert(topic.id(), gossip_ready_tx);
        self.topic_to_stream
            .entry(topic.clone())
            .and_modify(|stream_ids| stream_ids.push(stream_id))
            .or_insert(vec![stream_id]);
        self.topic_id_to_stream
            .entry(topic.id())
            .and_modify(|stream_ids| stream_ids.push(stream_id))
            .or_insert(vec![stream_id]);

        // Hot path: If we haven't joined a gossip overlay for this topic yet, optimistically try
        // to do it now. If this fails we should re-try sometime later using the
        // "try_join_pending_gossips" method.
        self.join_gossip(topic.id()).await?;

        // Spawn task to establish a channel for sending messages into gossip overlay.
        {
            let gossip_actor_tx = self.gossip_actor_tx.clone();
            let gossip_joined = self.gossip_joined.clone();
            tokio::task::spawn(async move {
                while let Some(event) = to_network_rx.recv().await {
                    let gossip_joined = gossip_joined.read().await;
                    if !gossip_joined.contains(&topic.id()) {
                        // If we haven't joined the gossip yet messages will be silently dropped
                        // here.
                        //
                        // For now this is fine as the user has two options:
                        //
                        // 1. They're combining sync with gossip. If the user stores all messages
                        //    before sending them (which they probably always should if they care
                        //    about consistency) sync will make sure that peers will catch up with
                        //    this data as soon as they connect to somebody.
                        // 2. They're don't care about consistency, but they are waiting for the
                        //    "gossip ready" signal before they send any messages.
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
                        // @TODO(adz): This fails silently right now, shouldn't this be propagated
                        // further to the user?
                        error!("failed broadcasting message to gossip for topic {topic:?}: {err}");
                        break;
                    }
                }
            });
        }

        Ok(())
    }

    /// Returns a list of all gossip topic ids we're interested in.
    pub fn topic_ids(&self) -> Vec<[u8; 32]> {
        self.subscribed
            .values()
            .map(|(topic, _)| topic.id())
            .collect()
    }

    /// Re-attempts joining pending gossip overlays for topic id's we haven't succeeded joining yet
    /// (for example because we lacked knowledge of other peers also being interested in them).
    ///
    /// This should ideally be called frequently by some other process or whenever we want to
    /// optimistically try to step forward with joining all overlays as fast as possible ("hot
    /// path").
    pub async fn try_join_pending_gossips(&self) -> Result<()> {
        for topic_id in self.gossip_pending.keys() {
            self.join_gossip(*topic_id).await?;
        }
        Ok(())
    }

    /// Mark that we've successfully joined a gossip overlay for this topic.
    pub async fn on_gossip_joined(&mut self, topic_id: [u8; 32]) {
        if let Some(ready_tx) = self.gossip_pending.remove(&topic_id) {
            let mut gossip_joined = self.gossip_joined.write().await;
            gossip_joined.insert(topic_id);

            // Inform local topic subscribers that the gossip overlay has been joined and is ready
            // for messages.
            if ready_tx.send(()).is_err() {
                warn!("gossip topic oneshot ready receiver dropped")
            }
        }
    }

    /// Attempt to join the gossip overlay for the given topic.
    async fn join_gossip(&self, topic_id: [u8; 32]) -> Result<()> {
        if self.has_joined_gossip(topic_id).await {
            return Ok(());
        }

        let peers = self
            .address_book
            .random_set(topic_id, JOIN_PEERS_SAMPLE_LEN)
            .await;

        if !peers.is_empty() {
            self.gossip_actor_tx
                .send(ToGossipActor::Join { topic_id, peers })
                .await?;
        }

        Ok(())
    }

    async fn has_joined_gossip(&self, topic_id: [u8; 32]) -> bool {
        let gossip_joined = self.gossip_joined.read().await;
        gossip_joined.contains(&topic_id)
    }

    /// Handle incoming messages from gossip.
    ///
    /// This method forwards messages to the subscribers for the given topic id.
    pub async fn on_gossip_message(
        &mut self,
        topic_id: [u8; 32],
        bytes: Vec<u8>,
        delivered_from: NodeId,
    ) -> Result<()> {
        if !self.has_joined_gossip(topic_id).await {
            warn!("received message for unknown topic {topic_id:?}");
            return Ok(());
        }

        // If there's currently a sync session running with that peer over that topic id we're
        // delaying delivery of these gossip messages and re-play them later after the session
        // finished.
        //
        // This reduces greatly the number of out-of-order messages in the stream and therefore the
        // pressure to re-order somewhere upstream.
        if let Some(buffer) = self.gossip_buffer.buffer(delivered_from, topic_id) {
            buffer.push(bytes);
            return Ok(());
        }

        // Different topics can be subscribed to the same gossip overlay, this is why we need to
        // multiplex the gossip message to potentially multiple streams.
        let stream_ids = self
            .topic_id_to_stream
            .get(&topic_id)
            .expect("consistent topic id to stream id mapping");
        for stream_id in stream_ids {
            let (_, from_network_tx) = self.subscribed.get(stream_id).expect("stream should exist");
            from_network_tx.send(FromNetwork::GossipMessage {
                bytes: bytes.clone(),
                delivered_from: to_public_key(delivered_from),
            })?;
        }

        Ok(())
    }

    /// Peers exchange topic ids in a process named "topic discovery". This method processes the
    /// learned topic id's from other peers.
    pub async fn on_discovered_topic_ids(
        &mut self,
        their_topic_ids: Vec<[u8; 32]>,
        delivered_from: NodeId,
    ) -> Result<()> {
        debug!(
            "learned about topic ids from {}: {:?}",
            delivered_from, their_topic_ids
        );

        // Inform the sync manager about any peer-topic combinations which are of interest to us.
        //
        // This queues up a sync session which will eventually request the data we are interested
        // in from that peer.
        let mut found_common_topic = false;
        if let Some(sync_actor_tx) = &self.sync_actor_tx {
            for (topic, _) in self.subscribed.values() {
                if their_topic_ids.contains(&topic.id()) {
                    found_common_topic = true;
                    let peer_topic = ToSyncActor::new(delivered_from, topic.clone());
                    sync_actor_tx.send(peer_topic).await?
                }
            }
        }

        // Hot path: Optimistically try to join gossip overlays at the same time.
        if found_common_topic {
            self.try_join_pending_gossips().await?;
        }

        Ok(())
    }

    /// Process new sync session starting with a peer over a topic.
    #[allow(unused_variables)]
    pub fn on_sync_start(&self, topic: T, node_id: NodeId) {
        // Do nothing here for now ..
    }

    /// Process handshake phase finishing during a sync session.
    ///
    /// In the handshake phase peers usually handle authorization and exchange the topic which will
    /// be synced.
    pub fn on_sync_handshake_success(&mut self, topic: T, node_id: NodeId) {
        self.gossip_buffer.lock(node_id, topic.id());
    }

    /// Process application-data message resulting from the sync session.
    pub fn on_sync_message(
        &mut self,
        topic: T,
        header: Vec<u8>,
        payload: Option<Vec<u8>>,
        delivered_from: NodeId,
    ) -> Result<()> {
        let stream_ids = self
            .topic_to_stream
            .get(&topic)
            .expect("consistent topic to stream id mapping");

        for stream_id in stream_ids {
            let (_, from_network_tx) = self.subscribed.get(stream_id).expect("stream should exist");
            from_network_tx.send(FromNetwork::SyncMessage {
                header: header.clone(),
                payload: payload.clone(),
                delivered_from: to_public_key(delivered_from),
            })?;
        }

        Ok(())
    }

    /// Process sync session finishing.
    pub async fn on_sync_done(&mut self, topic: T, node_id: NodeId) -> Result<()> {
        let topic_id = topic.id();
        let counter = self.gossip_buffer.unlock(node_id, topic_id);

        // If no locks are available anymore for that peer over that topic we can finally re-play
        // the gossip messages we've intercepted and kept around for the time of the sync session.
        if counter == 0 {
            let buffer = self
                .gossip_buffer
                .drain(node_id, topic_id)
                .expect("missing expected gossip buffer");

            for bytes in buffer {
                self.on_gossip_message(topic_id, bytes, node_id).await?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use futures_util::{FutureExt, StreamExt};
    use iroh_net::NodeAddr;
    use p2panda_core::PrivateKey;
    use p2panda_sync::Topic;
    use serde::{Deserialize, Serialize};
    use tokio::sync::{broadcast, mpsc, oneshot};
    use tokio_stream::wrappers::BroadcastStream;

    use crate::engine::AddressBook;
    use crate::network::FromNetwork;
    use crate::{to_public_key, TopicId};

    use super::TopicStreams;

    #[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
    enum TestTopic {
        Primary,
        Secondary,
    }

    impl Topic for TestTopic {}

    impl TopicId for TestTopic {
        fn id(&self) -> [u8; 32] {
            [0; 32]
        }
    }

    fn generate_node_id() -> NodeAddr {
        let private_key = PrivateKey::new();
        let public_key = private_key.public_key();
        let bytes = public_key.as_bytes();
        NodeAddr::new(iroh_net::NodeId::from_bytes(bytes).unwrap())
    }

    #[tokio::test]
    async fn ooo_gossip_buffering() {
        let (gossip_actor_tx, _gossip_actor_rx) = mpsc::channel(128);
        let (sync_actor_tx, _sync_actor_rx) = mpsc::channel(128);
        let (from_network_tx, from_network_rx) = broadcast::channel(128);
        let (_to_network_tx, to_network_rx) = mpsc::channel(128);
        let (gossip_ready_tx, _) = oneshot::channel();
        let mut from_network_rx_stream = BroadcastStream::new(from_network_rx);

        let topic = TestTopic::Primary;
        let topic_id = topic.id();

        let mut address_book = AddressBook::new([1; 32]);

        let peer_1 = generate_node_id();
        address_book.add_peer(peer_1.clone()).await;
        address_book.add_topic_id(peer_1.node_id, topic.id()).await;

        let mut topic_streams =
            TopicStreams::<TestTopic>::new(gossip_actor_tx, address_book, Some(sync_actor_tx));

        topic_streams
            .subscribe(
                topic.clone(),
                from_network_tx,
                to_network_rx,
                gossip_ready_tx,
            )
            .await
            .unwrap();

        topic_streams.on_gossip_joined(topic_id).await;

        topic_streams.on_sync_start(topic.clone(), peer_1.node_id);
        topic_streams.on_sync_handshake_success(topic.clone(), peer_1.node_id);

        topic_streams
            .on_gossip_message(topic_id, b"a new cmos battery".to_vec(), peer_1.node_id)
            .await
            .unwrap();
        topic_streams
            .on_gossip_message(topic_id, b"and icecream".to_vec(), peer_1.node_id)
            .await
            .unwrap();

        assert!(
            from_network_rx_stream.next().now_or_never().is_none(),
            "stream does not contain any messages yet from gossip"
        );

        topic_streams
            .on_sync_done(topic, peer_1.node_id)
            .await
            .unwrap();

        assert_eq!(
            from_network_rx_stream.next().await.unwrap().unwrap(),
            FromNetwork::GossipMessage {
                bytes: b"a new cmos battery".to_vec(),
                delivered_from: to_public_key(peer_1.node_id),
            }
        );
        assert_eq!(
            from_network_rx_stream.next().await.unwrap().unwrap(),
            FromNetwork::GossipMessage {
                bytes: b"and icecream".to_vec(),
                delivered_from: to_public_key(peer_1.node_id),
            }
        );
    }
}
