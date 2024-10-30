// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use iroh_net::NodeId;
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
}

impl AddressBook {
    /// Returns an empty address book for this network.
    pub fn new(network_id: NetworkId) -> Self {
        Self {
            network_id,
            inner: Arc::new(RwLock::new(AddressBookInner {
                known_peer_topic_ids: HashMap::new(),
            })),
        }
    }

    pub async fn add_peer(&mut self, node_id: NodeId) -> bool {
        self.add_topic_id(node_id, self.network_id).await
    }

    pub async fn add_topic_id(&mut self, node_id: NodeId, topic_id: [u8; 32]) -> bool {
        let mut inner = self.inner.write().await;

        if let Some(known_topics) = inner.known_peer_topic_ids.get_mut(&node_id) {
            return known_topics.insert(topic_id);
        }

        let mut topics = HashSet::new();
        topics.insert(topic_id);
        inner.known_peer_topic_ids.insert(node_id, topics);
        true
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
