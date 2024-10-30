// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use iroh_net::{NodeAddr, NodeId};
use rand::seq::IteratorRandom;
use tokio::sync::RwLock;

use crate::NetworkId;

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
    /// Returns an empty address book for this network.
    pub fn new(network_id: NetworkId) -> Self {
        Self {
            network_id,
            inner: Arc::new(RwLock::new(AddressBookInner {
                known_peer_topic_ids: HashMap::new(),
                known_peer_addresses: HashMap::new(),
            })),
        }
    }

    pub async fn add_peer(&mut self, node_addr: NodeAddr) {
        let node_id = node_addr.node_id;
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
