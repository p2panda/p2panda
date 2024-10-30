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

type TopicStream<T> = (T, broadcast::Sender<FromNetwork>);

pub type TopicStreamId = usize;

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

    pub async fn subscribe(
        &mut self,
        topic: T,
        from_network_tx: broadcast::Sender<FromNetwork>,
        mut to_network_rx: mpsc::Receiver<ToNetwork>,
        gossip_ready_tx: oneshot::Sender<()>,
    ) -> Result<()> {
        let stream_id = self.next_stream_id;
        self.next_stream_id += 1;

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

        Ok(())
    }

    pub fn topic_ids(&self) -> Vec<[u8; 32]> {
        self.subscribed
            .values()
            .map(|(topic, _)| topic.id())
            .collect()
    }

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
    /// This method forwards messages to the subscribers for the given topic.
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

        if let Some(buffer) = self.gossip_buffer.buffer(delivered_from, topic_id) {
            buffer.push(bytes);
            return Ok(());
        }

        let stream_ids = self
            .topic_id_to_stream
            .get(&topic_id)
            .expect("consistent topic id to stream id mapping");

        // Different topics can be subscribed to the same gossip overlay, this is why we need to
        // multiplex the gossip message to potentially multiple streams.
        for stream_id in stream_ids {
            let (_, from_network_tx) = self.subscribed.get(stream_id).expect("stream should exist");
            from_network_tx.send(FromNetwork::GossipMessage {
                bytes: bytes.clone(),
                delivered_from: to_public_key(delivered_from),
            })?;
        }

        Ok(())
    }

    /// Process "topics of interest" from another peer.
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
        if let Some(sync_actor_tx) = &self.sync_actor_tx {
            for (_, (topic, _)) in &self.subscribed {
                if their_topic_ids.contains(&topic.id()) {
                    let peer_topic = ToSyncActor::new(delivered_from, topic.clone());
                    sync_actor_tx.send(peer_topic).await?
                }
            }
        }

        // Hot path: Optimistically try to join gossip overlays at the same time.
        self.try_join_pending_gossips().await?;

        Ok(())
    }

    #[allow(unused_variables)]
    pub fn on_sync_start(&self, topic: T, node_id: NodeId) {
        // Do nothing here for now ..
    }

    pub fn on_sync_handshake_success(&mut self, topic: T, node_id: NodeId) {
        self.gossip_buffer.lock(node_id, topic.id());
    }

    /// Process message forwarded from the sync actor.
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

    pub async fn on_sync_done(&mut self, topic: T, node_id: NodeId) -> Result<()> {
        let topic_id = topic.id();
        let counter = self.gossip_buffer.unlock(node_id, topic_id);

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
