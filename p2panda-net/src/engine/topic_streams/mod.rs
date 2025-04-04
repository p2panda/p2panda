// SPDX-License-Identifier: MIT OR Apache-2.0

mod receiver;
mod sender;

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use p2panda_core::PublicKey;
use p2panda_sync::TopicQuery;
use tokio::sync::{RwLock, mpsc, oneshot};
use tracing::{debug, error, warn};

use crate::TopicId;
use crate::engine::address_book::AddressBook;
use crate::engine::constants::JOIN_PEERS_SAMPLE_LEN;
use crate::engine::engine::ToEngineActor;
use crate::engine::gossip::ToGossipActor;
use crate::engine::gossip_buffer::GossipBuffer;
use crate::network::{FromNetwork, ToNetwork};
use crate::sync::manager::ToSyncActor;

pub use crate::engine::topic_streams::receiver::{TopicReceiver, TopicReceiverStream};
pub use crate::engine::topic_streams::sender::TopicSender;

/// Managed data stream over an application-defined topic.
type TopicStream<T> = (T, mpsc::Sender<FromNetwork>);

/// Every stream has a unique identifier.
type TopicStreamId = usize;

/// Possible halves of a split channel.
#[derive(Debug, PartialEq)]
pub enum TopicChannelType {
    Sender,
    Receiver,
}

/// State of two halves of a topic channel.
#[derive(Debug, PartialEq)]
struct TopicStreamState {
    sender: bool,
    receiver: bool,
}

impl TopicStreamState {
    fn new() -> Self {
        Self {
            sender: true,
            receiver: true,
        }
    }

    /// The topic channel is still active if either the sender, receiver or both are active.
    fn is_active(&self) -> bool {
        self.sender || self.receiver
    }
}

/// Manages subscriptions to topics in form of data streams.
///
/// A stream has quite a bit of state to deal with, this includes:
///
/// 1. Try to enter a gossip overlay for sending messages in "live mode" over a topic id.
/// 2. Help the sync manager with learning about topics of interest and guide it to connect to
///    peers for syncing up state with them.
/// 3. Intercept and temporarily buffer incoming gossip messages of a peer when we're currently in
///    a sync session with them. As soon as this sync session has finished we can re-play the
///    messages. This helps reducing the number of out-of-order messages.
/// 4. Applications can subscribe to topics multiple times, or to different topics but with the
///    same topic ids. This stream handler multiplexes messages to the right place, even when
///    there's duplicates.
/// 5. Unsubscribe and clean up when all senders and receivers for a topic have been dropped.
#[derive(Debug)]
pub struct TopicStreams<T> {
    next_stream_id: usize,
    address_book: AddressBook,
    gossip_buffer: GossipBuffer,
    engine_actor_tx: mpsc::Sender<ToEngineActor<T>>,
    gossip_actor_tx: mpsc::Sender<ToGossipActor>,
    sync_actor_tx: Option<mpsc::Sender<ToSyncActor<T>>>,
    gossip_joined: Arc<RwLock<HashMap<[u8; 32], usize>>>,
    gossip_pending: HashMap<[u8; 32], oneshot::Sender<()>>,
    subscribed: HashMap<TopicStreamId, TopicStream<T>>,
    active_streams: HashMap<TopicStreamId, TopicStreamState>,
    topic_id_to_stream: HashMap<[u8; 32], Vec<TopicStreamId>>,
    topic_to_stream: HashMap<T, Vec<TopicStreamId>>,
}

impl<T> TopicStreams<T>
where
    T: TopicQuery + TopicId + 'static,
{
    pub fn new(
        address_book: AddressBook,
        engine_actor_tx: mpsc::Sender<ToEngineActor<T>>,
        gossip_actor_tx: mpsc::Sender<ToGossipActor>,
        sync_actor_tx: Option<mpsc::Sender<ToSyncActor<T>>>,
    ) -> Self {
        Self {
            next_stream_id: 1,
            address_book,
            gossip_buffer: Default::default(),
            engine_actor_tx,
            gossip_actor_tx,
            sync_actor_tx,
            gossip_joined: Arc::new(RwLock::new(HashMap::new())),
            gossip_pending: HashMap::new(),
            subscribed: HashMap::new(),
            active_streams: HashMap::new(),
            topic_id_to_stream: HashMap::new(),
            topic_to_stream: HashMap::new(),
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
        topic_sender_tx: oneshot::Sender<TopicSender<T>>,
        topic_receiver_tx: oneshot::Sender<TopicReceiver<T>>,
        gossip_ready_tx: oneshot::Sender<()>,
    ) -> Result<()> {
        let (to_network_tx, mut to_network_rx) = mpsc::channel::<ToNetwork>(128);
        let (from_network_tx, from_network_rx) = mpsc::channel::<FromNetwork>(128);

        // Every subscription stream receives its own unique identifier.
        let stream_id = self.next_stream_id;
        self.next_stream_id += 1;

        // Create two parts of a topic stream channel. These are used to send bytes into the
        // network and receive bytes out of the network. Unsubscribe "clean up" actions will
        // be triggered when both the sender and receiver have been dropped.
        let topic_tx = TopicSender::new(
            topic.clone(),
            stream_id,
            to_network_tx,
            self.engine_actor_tx.clone(),
        )
        .await;
        let topic_rx = TopicReceiver::new(
            topic.clone(),
            stream_id,
            from_network_rx,
            self.engine_actor_tx.clone(),
        )
        .await;

        // Send the topic stream channels back to the subscriber.
        if topic_sender_tx.send(topic_tx).is_err() {
            warn!("topic stream sender oneshot receiver dropped")
        }
        if topic_receiver_tx.send(topic_rx).is_err() {
            warn!("topic stream receiver oneshot receiver dropped")
        }

        // Track the state of the topic stream channel.
        //
        // This allows us to only initiate unsubscribe logic for a topic once both the sender and
        // receiver have been dropped.
        self.active_streams
            .insert(stream_id, TopicStreamState::new());

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
                    if !gossip_joined.contains_key(&topic.id()) {
                        // If we haven't joined the gossip yet messages will be silently dropped
                        // here.
                        //
                        // For now this is fine as the user has two options:
                        //
                        // 1. They're combining sync with gossip. If the user stores all messages
                        //    before sending them (which they probably always should if they care
                        //    about consistency) sync will make sure that peers will catch up with
                        //    this data as soon as they connect to somebody.
                        // 2. They don't care about consistency, but are waiting for the
                        //    "gossip ready" signal before sending any messages.
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

    /// Cleans up all state for the stream associated with the given topic ID once the sender and
    /// receiver have both been dropped.
    ///
    /// A single stream can be unsubscribed from without affecting any other active streams for the
    /// same topic ID.
    ///
    /// Returns `true` if the unsubscribe process is complete for the given stream ID. This
    /// is only the case once this method has been called for both the sender and receiver
    /// of the stream.
    ///
    /// Returns `false` if the unsubscribe process is not yet complete for the given stream ID.
    pub async fn unsubscribe(
        &mut self,
        topic_id: [u8; 32],
        stream_id: usize,
        channel_type: TopicChannelType,
    ) -> Result<bool> {
        let mut unsubscribe_is_complete = false;

        // Update the channel state for this stream.
        if let Some(channel_state) = self.active_streams.get_mut(&stream_id) {
            match channel_type {
                TopicChannelType::Sender => channel_state.sender = false,
                TopicChannelType::Receiver => channel_state.receiver = false,
            }
        }

        if let Some(stream) = self.active_streams.get(&stream_id) {
            // Only execute clean up logic if both the sender and receiver have been dropped.
            if !stream.is_active() {
                self.active_streams.remove(&stream_id);

                let _ = self.subscribed.remove(&stream_id);
                for (_topic, streams) in self.topic_to_stream.iter_mut() {
                    streams.retain(|&id| id != stream_id)
                }
                for (_topic, streams) in self.topic_id_to_stream.iter_mut() {
                    streams.retain(|&id| id != stream_id)
                }
                self.topic_to_stream
                    .retain(|_, streams| !streams.is_empty());
                self.topic_id_to_stream
                    .retain(|_, streams| !streams.is_empty());

                // @TODO(glyph): We can't currently remove the topic-stream mapping of
                // `self.topic_to_stream` when the topic maps to an empty vector because we don't
                // receive the `Topic` as input to `unsubscribe()`.

                let mut gossip_joined = self.gossip_joined.write().await;
                if let Some(counter) = gossip_joined.get_mut(&topic_id) {
                    if *counter != 0 {
                        *counter -= 1
                    }
                }

                // If the counter has reached zero that means that no more active subscribers remain for
                // this topic id and we can remove it completely from the set.
                //
                // All topics from `joined` get moved to `pending` when we reset; this would result in
                // unsubscribed topics being erroneously re-joined. So we need them to be removed.
                if let Some(0) = gossip_joined.get(&topic_id) {
                    gossip_joined.remove(&topic_id);
                }

                unsubscribe_is_complete = true;
            }
        }

        Ok(unsubscribe_is_complete)
    }

    /// Returns a list of all gossip topic ids we're interested in.
    pub fn topic_ids(&self) -> Vec<[u8; 32]> {
        self.subscribed
            .values()
            .map(|(topic, _)| topic.id())
            .collect()
    }

    /// Moves all gossip topics which were previously joined into the set of pending joins.
    ///
    /// This is useful for rejoining gossip topic overlays after an extended loss of network
    /// connectivity. One important consideration is that the ready receiver is immediately
    /// dropped, meaning that the application layer is never made aware when the topic has been
    /// rejoined.
    pub async fn move_joined_to_pending(&mut self) {
        let mut gossip_joined = self.gossip_joined.write().await;
        for (topic, _counter) in gossip_joined.drain() {
            let (ready_tx, _ready_rx) = oneshot::channel();
            self.gossip_pending.insert(topic, ready_tx);
        }
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
            if let Some(counter) = gossip_joined.get_mut(&topic_id) {
                *counter += 1;
            } else {
                gossip_joined.insert(topic_id, 1);
            }

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
        gossip_joined.contains_key(&topic_id)
    }

    /// Handle incoming messages from gossip.
    ///
    /// This method forwards messages to the subscribers for the given topic id.
    pub async fn on_gossip_message(
        &mut self,
        topic_id: [u8; 32],
        bytes: Vec<u8>,
        delivered_from: PublicKey,
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
            from_network_tx
                .send(FromNetwork::GossipMessage {
                    bytes: bytes.clone(),
                    delivered_from,
                })
                .await?;
        }

        Ok(())
    }

    /// Peers exchange topic ids in a process named "topic discovery". This method processes the
    /// learned topic id's from other peers.
    pub async fn on_discovered_topic_ids(
        &mut self,
        their_topic_ids: Vec<[u8; 32]>,
        peer: PublicKey,
    ) -> Result<()> {
        debug!("learned about topic ids of {}: {:?}", peer, their_topic_ids);

        // Inform the sync manager about any peer-topic combinations which are of interest to us.
        //
        // This queues up a sync session which will eventually request the data we are interested
        // in from that peer.
        let mut found_common_topic = false;
        if let Some(sync_actor_tx) = &self.sync_actor_tx {
            for (topic, _) in self.subscribed.values() {
                if their_topic_ids.contains(&topic.id()) {
                    found_common_topic = true;
                    let peer_topic = ToSyncActor::new_discovery(peer, topic.clone());
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

    /// Process new sync session starting with a peer.
    ///
    /// If a topic is known we've initiated the sync session. If it is `None` we accepted a sync
    /// session and still need to learn about the topic (see `on_sync_handshake_success`).
    #[allow(unused_variables)]
    pub fn on_sync_start(&self, topic: Option<T>, peer: PublicKey) {
        // Do nothing here for now ..
    }

    /// Process handshake phase finishing during a sync session.
    ///
    /// In the handshake phase peers usually handle authorization and exchange the topic which will
    /// be synced.
    pub fn on_sync_handshake_success(&mut self, topic: T, peer: PublicKey) {
        self.gossip_buffer.lock(peer, topic.id());
    }

    /// Process application-data message resulting from the sync session.
    pub async fn on_sync_message(
        &mut self,
        topic: T,
        header: Vec<u8>,
        payload: Option<Vec<u8>>,
        delivered_from: PublicKey,
    ) -> Result<()> {
        let stream_ids = self
            .topic_to_stream
            .get(&topic)
            .expect("consistent topic to stream id mapping");

        for stream_id in stream_ids {
            let (_, from_network_tx) = self.subscribed.get(stream_id).expect("stream should exist");
            from_network_tx
                .send(FromNetwork::SyncMessage {
                    header: header.clone(),
                    payload: payload.clone(),
                    delivered_from,
                })
                .await?;
        }

        Ok(())
    }

    /// Process sync session finishing.
    pub async fn on_sync_done(&mut self, topic: T, peer: PublicKey) -> Result<()> {
        let topic_id = topic.id();
        if let Some(counter) = self.gossip_buffer.unlock(peer, topic_id) {
            // If no locks are available anymore for that peer over that topic we can finally re-play
            // the gossip messages we've intercepted and kept around for the time of the sync session.
            if counter == 0 {
                let buffer = self
                    .gossip_buffer
                    .drain(peer, topic_id)
                    .expect("missing expected gossip buffer");

                for bytes in buffer {
                    self.on_gossip_message(topic_id, bytes, peer).await?;
                }
            }
        }

        Ok(())
    }

    /// Process sync session failure by draining the associated gossip buffer.
    pub async fn on_sync_failed(&mut self, topic: Option<T>, peer: PublicKey) -> Result<()> {
        // If we already learned about a topic during the sync handshake phase when this error took
        // place we likely have opened up a gossip message buffer already, so we should make sure
        // to close it here.
        if let Some(topic) = topic {
            let topic_id = topic.id();
            if let Some(counter) = self.gossip_buffer.unlock(peer, topic_id) {
                // If no locks are available anymore for that peer over that topic we can drain the gossip
                // messages from the buffer and drop them.
                if counter == 0 {
                    self.gossip_buffer
                        .drain(peer, topic_id)
                        .expect("missing expected gossip buffer");
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use futures_util::{FutureExt, StreamExt};
    use p2panda_core::PrivateKey;
    use p2panda_sync::TopicQuery;
    use serde::{Deserialize, Serialize};
    use tokio::sync::{mpsc, oneshot};
    use tokio::time::timeout;

    use crate::engine::{AddressBook, ToEngineActor};

    use crate::network::FromNetwork;
    use crate::{NodeAddress, TopicId};

    use super::{TopicChannelType, TopicReceiverStream, TopicStreams};

    #[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
    enum TestTopic {
        Primary,
        Secondary,
    }

    impl TopicQuery for TestTopic {}

    impl TopicId for TestTopic {
        fn id(&self) -> [u8; 32] {
            [0; 32]
        }
    }

    fn generate_node_addr() -> NodeAddress {
        let private_key = PrivateKey::new();
        NodeAddress::from_public_key(private_key.public_key())
    }

    #[tokio::test]
    async fn ooo_gossip_buffering() {
        let (engine_actor_tx, _engine_actor_rx) = mpsc::channel(128);
        let (gossip_actor_tx, _gossip_actor_rx) = mpsc::channel(128);
        let (sync_actor_tx, _sync_actor_rx) = mpsc::channel(128);

        let (gossip_ready_tx, _) = oneshot::channel();
        let (topic_stream_sender_tx, _topic_stream_sender_rx) = oneshot::channel();
        let (topic_stream_receiver_tx, topic_stream_receiver_rx) = oneshot::channel();

        let topic = TestTopic::Primary;
        let topic_id = topic.id();

        let mut address_book = AddressBook::new([1; 32]);

        let peer_1 = generate_node_addr();
        address_book.add_peer(peer_1.clone()).await;
        address_book
            .add_topic_id(peer_1.public_key, topic.id())
            .await;

        let mut topic_streams = TopicStreams::<TestTopic>::new(
            address_book,
            engine_actor_tx,
            gossip_actor_tx,
            Some(sync_actor_tx),
        );

        topic_streams
            .subscribe(
                topic.clone(),
                topic_stream_sender_tx,
                topic_stream_receiver_tx,
                gossip_ready_tx,
            )
            .await
            .unwrap();

        let from_network_rx = topic_stream_receiver_rx.await.unwrap();
        let mut from_network_rx_stream = TopicReceiverStream::new(from_network_rx);

        topic_streams.on_gossip_joined(topic_id).await;

        topic_streams.on_sync_start(Some(topic.clone()), peer_1.public_key);
        topic_streams.on_sync_handshake_success(topic.clone(), peer_1.public_key);

        topic_streams
            .on_gossip_message(topic_id, b"a new cmos battery".to_vec(), peer_1.public_key)
            .await
            .unwrap();
        topic_streams
            .on_gossip_message(topic_id, b"and icecream".to_vec(), peer_1.public_key)
            .await
            .unwrap();

        assert!(
            from_network_rx_stream.next().now_or_never().is_none(),
            "stream does not contain any messages yet from gossip"
        );

        topic_streams
            .on_sync_done(topic, peer_1.public_key)
            .await
            .unwrap();

        assert_eq!(
            from_network_rx_stream.next().await.unwrap(),
            FromNetwork::GossipMessage {
                bytes: b"a new cmos battery".to_vec(),
                delivered_from: peer_1.public_key,
            }
        );
        assert_eq!(
            from_network_rx_stream.next().await.unwrap(),
            FromNetwork::GossipMessage {
                bytes: b"and icecream".to_vec(),
                delivered_from: peer_1.public_key,
            }
        );
    }

    #[tokio::test]
    async fn subscribe() {
        let (engine_actor_tx, _engine_actor_rx) = mpsc::channel(128);
        let (gossip_actor_tx, _gossip_actor_rx) = mpsc::channel(128);
        let (sync_actor_tx, _sync_actor_rx) = mpsc::channel(128);

        let (gossip_ready_tx, gossip_ready_rx) = oneshot::channel();
        let (topic_stream_sender_tx, _topic_stream_sender_rx) = oneshot::channel();
        let (topic_stream_receiver_tx, _topic_stream_receiver_rx) = oneshot::channel();

        let topic = TestTopic::Primary;
        let topic_id = topic.id();

        let mut address_book = AddressBook::new([1; 32]);

        let peer_1 = generate_node_addr();
        address_book.add_peer(peer_1.clone()).await;
        address_book
            .add_topic_id(peer_1.public_key, topic.id())
            .await;

        let mut topic_streams = TopicStreams::<TestTopic>::new(
            address_book,
            engine_actor_tx,
            gossip_actor_tx,
            Some(sync_actor_tx),
        );

        let current_stream_id = topic_streams.next_stream_id;

        // Subscribe to the topic:

        topic_streams
            .subscribe(
                topic.clone(),
                topic_stream_sender_tx,
                topic_stream_receiver_tx,
                gossip_ready_tx,
            )
            .await
            .unwrap();

        // Ensure the correct post-subscribe state:

        assert_eq!(topic_streams.next_stream_id, 2);
        assert!(topic_streams.gossip_pending.contains_key(&topic_id));
        assert!(
            topic_streams
                .active_streams
                .contains_key(&current_stream_id)
        );

        let stream_state = topic_streams
            .active_streams
            .get(&current_stream_id)
            .unwrap();
        assert_eq!(stream_state.sender, true);
        assert_eq!(stream_state.receiver, true);

        assert!(topic_streams.topic_id_to_stream.contains_key(&topic_id));
        assert!(
            topic_streams
                .topic_id_to_stream
                .get(&topic_id)
                .unwrap()
                .contains(&current_stream_id)
        );

        assert!(topic_streams.topic_to_stream.contains_key(&topic));
        assert!(
            topic_streams
                .topic_to_stream
                .get(&topic)
                .unwrap()
                .contains(&current_stream_id)
        );

        // Process the joining of the gossip topic:

        topic_streams.on_gossip_joined(topic_id).await;
        if let Err(_) = timeout(Duration::from_millis(10), gossip_ready_rx).await {
            panic!("did not receive gossip ready signal within 10 ms");
        }

        // Ensure the correct post-joined state:

        assert!(!topic_streams.gossip_pending.contains_key(&topic_id));
        let gossip_joined = topic_streams.gossip_joined.read().await;
        assert!(gossip_joined.contains_key(&topic_id));
        assert_eq!(gossip_joined.get(&topic_id).unwrap(), &1);
    }

    #[tokio::test]
    async fn unsubscribe_on_drop() {
        let (engine_actor_tx, mut engine_actor_rx) = mpsc::channel(128);
        let (gossip_actor_tx, _gossip_actor_rx) = mpsc::channel(128);
        let (sync_actor_tx, _sync_actor_rx) = mpsc::channel(128);

        let (gossip_ready_tx, _gossip_ready_rx) = oneshot::channel();
        let (topic_stream_sender_tx, topic_stream_sender_rx) = oneshot::channel();
        let (topic_stream_receiver_tx, topic_stream_receiver_rx) = oneshot::channel();

        let topic = TestTopic::Primary;
        let topic_id = topic.id();

        let mut address_book = AddressBook::new([1; 32]);

        let peer_1 = generate_node_addr();
        address_book.add_peer(peer_1.clone()).await;
        address_book
            .add_topic_id(peer_1.public_key, topic.id())
            .await;

        let mut topic_streams = TopicStreams::<TestTopic>::new(
            address_book,
            engine_actor_tx,
            gossip_actor_tx,
            Some(sync_actor_tx),
        );

        let current_stream_id = topic_streams.next_stream_id;

        // Subscribe to the topic:

        topic_streams
            .subscribe(
                topic.clone(),
                topic_stream_sender_tx,
                topic_stream_receiver_tx,
                gossip_ready_tx,
            )
            .await
            .unwrap();

        let to_network_tx = topic_stream_sender_rx.await.unwrap();
        let from_network_rx = topic_stream_receiver_rx.await.unwrap();

        topic_streams.on_gossip_joined(topic_id).await;

        // Ensure the correct post-unsubscribe state:

        // Drop sender.
        drop(to_network_tx);

        if let Some(ToEngineActor::UnsubscribeTopic {
            topic: received_topic,
            stream_id,
            channel_type,
        }) = engine_actor_rx.recv().await
        {
            assert_eq!(received_topic, topic);
            assert_eq!(stream_id, current_stream_id);
            assert_eq!(channel_type, TopicChannelType::Sender);
        } else {
            panic!("expected to receive unsubscribe topic event on engine actor receiver")
        }

        // Unsubscribe the sender.
        let unsubscribe_is_complete = topic_streams
            .unsubscribe(topic_id, current_stream_id, TopicChannelType::Sender)
            .await
            .unwrap();
        assert!(!unsubscribe_is_complete);

        let stream_state = topic_streams
            .active_streams
            .get(&current_stream_id)
            .unwrap();
        assert_eq!(stream_state.sender, false);
        assert_eq!(stream_state.receiver, true);

        assert!(topic_streams.topic_id_to_stream.contains_key(&topic_id));
        assert!(
            topic_streams
                .topic_id_to_stream
                .get(&topic_id)
                .unwrap()
                .contains(&current_stream_id)
        );

        assert!(topic_streams.topic_to_stream.contains_key(&topic));
        assert!(
            topic_streams
                .topic_to_stream
                .get(&topic)
                .unwrap()
                .contains(&current_stream_id)
        );

        // Drop receiver.
        drop(from_network_rx);

        if let Some(ToEngineActor::UnsubscribeTopic {
            topic: received_topic,
            stream_id,
            channel_type,
        }) = engine_actor_rx.recv().await
        {
            assert_eq!(received_topic, topic);
            assert_eq!(stream_id, current_stream_id);
            assert_eq!(channel_type, TopicChannelType::Receiver);
        } else {
            panic!("expected to receive unsubscribe topic event on engine actor receiver")
        }

        // Unsubscribe the receiver.
        let unsubscribe_is_complete = topic_streams
            .unsubscribe(topic_id, current_stream_id, TopicChannelType::Receiver)
            .await
            .unwrap();
        assert!(unsubscribe_is_complete);

        assert!(
            !topic_streams
                .active_streams
                .contains_key(&current_stream_id)
        );

        assert!(!topic_streams.topic_id_to_stream.contains_key(&topic_id));
        assert!(topic_streams.topic_to_stream.is_empty());

        assert!(!topic_streams.gossip_pending.contains_key(&topic_id));
        let gossip_joined = topic_streams.gossip_joined.read().await;
        assert!(!gossip_joined.contains_key(&topic_id));
    }
}
