// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;

use iroh_net::key::PublicKey;
use iroh_net::{NodeAddr, NodeId};
use rand::seq::IteratorRandom;

pub struct PeerMap {
    pub known_peers: HashMap<NodeId, NodeAddr>,
    pub topics: HashMap<[u8; 32], Vec<PublicKey>>,
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
