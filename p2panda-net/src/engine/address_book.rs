// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use p2panda_core::PublicKey;
use rand::seq::IteratorRandom;
use tokio::sync::RwLock;

use crate::{NetworkId, NodeAddress};

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
    known_peer_topic_ids: HashMap<PublicKey, HashSet<[u8; 32]>>,
    known_peer_addresses: HashMap<PublicKey, HashSet<NodeAddress>>,
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
    pub async fn add_peer(&mut self, node_addr: NodeAddress) {
        let public_key = node_addr.public_key;

        // Every peer in this network is automatically part of the network-wide gossip overlay
        // which is used for topic discovery.
        self.add_topic_id(public_key, self.network_id).await;

        let mut inner = self.inner.write().await;
        inner
            .known_peer_addresses
            .entry(public_key)
            .or_default()
            .insert(node_addr);
    }

    /// Associate peer with a topic id they are interested in.
    pub async fn add_topic_id(&mut self, public_key: PublicKey, topic_id: [u8; 32]) {
        let mut inner = self.inner.write().await;
        inner
            .known_peer_topic_ids
            .entry(public_key)
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
    pub async fn known_peers(&self) -> Vec<NodeAddress> {
        let inner = self.inner.read().await;
        inner
            .known_peer_addresses
            .values()
            .flatten()
            .cloned()
            .collect()
    }

    /// Return random set of known peers with an interest in the given topic.
    pub async fn random_set(&self, topic_id: [u8; 32], sample_len: usize) -> Vec<PublicKey> {
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

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

    use p2panda_core::PrivateKey;

    use crate::NodeAddress;

    use super::AddressBook;

    #[tokio::test]
    async fn add_peer_without_duplication() {
        let private_key = PrivateKey::new();
        let public_key = private_key.public_key();

        let network_id = [3; 32];

        let mut address_book = AddressBook::new(network_id);

        // Create a node address.
        let mut node_addr = NodeAddress::from_public_key(public_key);
        let socket_addr_v4 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        node_addr.direct_addresses = vec![socket_addr_v4];

        // Add peer address and ensure a single peer is known.
        address_book.add_peer(node_addr.clone()).await;
        let known_peers = address_book.known_peers().await;
        assert_eq!(known_peers.len(), 1);

        let socket_addr_v6 = SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 0);
        node_addr.direct_addresses.push(socket_addr_v6);

        // Add a second peer address and ensure two peers are known.
        address_book.add_peer(node_addr.clone()).await;
        let known_peers = address_book.known_peers().await;
        assert_eq!(known_peers.len(), 2);

        // Add the second peer address again and ensure no duplication occurs.
        address_book.add_peer(node_addr.clone()).await;
        let known_peers = address_book.known_peers().await;
        assert_eq!(known_peers.len(), 2);
    }
}
