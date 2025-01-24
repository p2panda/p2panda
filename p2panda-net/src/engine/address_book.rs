// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use iroh::{NodeAddr, NodeId};
use rand::seq::IteratorRandom;
use tokio::sync::RwLock;

use crate::NetworkId;

/// Address book with peer addresses and their topic ids.
///
/// Manages a list of all peer addresses which are known to us (usually populated by a "peer
/// discovery" process) and a list of all topic id's peers in this network are interested in
/// (usually populated by a "topic discovery" process).
#[derive(Debug, Clone)]
pub struct AddressBook {
    network_id: NetworkId,
    inner: Arc<RwLock<AddressBookInner>>,
}

#[derive(Debug)]
struct AddressBookInner {
    known_peer_topic_ids: HashMap<NodeId, HashSet<[u8; 32]>>,
    known_peer_addresses: HashMap<NodeId, Vec<NodeAddr>>,
}

impl AddressBook {
    /// Return an empty address book for this network.
    pub fn new(network_id: NetworkId) -> Self {
        Self {
            network_id,
            inner: Arc::new(RwLock::new(AddressBookInner {
                known_peer_topic_ids: HashMap::new(),
                known_peer_addresses: HashMap::new(),
            })),
        }
    }

    /// Add or update peer address to the address book.
    pub async fn add_peer(&mut self, node_addr: NodeAddr) {
        let node_id = node_addr.node_id;

        // Every peer in this network is automatically part of the network-wide gossip overlay
        // which is used for topic discovery.
        self.add_topic_id(node_id, self.network_id).await;

        let mut inner = self.inner.write().await;
        inner
            .known_peer_addresses
            .entry(node_id)
            .and_modify(|addrs| {
                addrs.push(node_addr.clone());
            })
            .or_insert(vec![node_addr]);
    }

    /// Associate peer with a topic id they are interested in.
    pub async fn add_topic_id(&mut self, node_id: NodeId, topic_id: [u8; 32]) {
        let mut inner = self.inner.write().await;
        inner
            .known_peer_topic_ids
            .entry(node_id)
            .and_modify(|known_topics| {
                known_topics.insert(topic_id);
            })
            .or_insert({
                let mut topics = HashSet::new();
                topics.insert(topic_id);
                topics
            });
    }

    /// Return list of all currently known peer addresses.
    pub async fn known_peers(&self) -> Vec<NodeAddr> {
        let inner = self.inner.read().await;
        inner
            .known_peer_addresses
            .values()
            .flatten()
            .cloned()
            .collect()
    }

    /// Return random set of known peers with an interest in the given topic.
    pub async fn random_set(&self, topic_id: [u8; 32], sample_len: usize) -> Vec<NodeId> {
        let inner = self.inner.read().await;

        let nodes_interested_in_topic =
            inner
                .known_peer_topic_ids
                .iter()
                .fold(Vec::new(), |mut acc, (node_id, topics)| {
                    if topics.contains(&topic_id) {
                        acc.push(*node_id);
                    }
                    acc
                });

        nodes_interested_in_topic
            .iter()
            .choose_multiple(&mut rand::thread_rng(), sample_len)
            .into_iter()
            .cloned()
            .collect()
    }
}
