// SPDX-License-Identifier: MIT OR Apache-2.0

use anyhow::{Context, Result, bail};
use p2panda_core::{PrivateKey, PublicKey, Signature};
use rand::random;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::NetworkId;
use crate::bytes::{FromBytes, ToBytes};
use crate::engine::address_book::AddressBook;
use crate::engine::constants::JOIN_PEERS_SAMPLE_LEN;
use crate::engine::gossip::ToGossipActor;

#[derive(Debug, Default, PartialEq, Eq)]
enum Status {
    #[default]
    Idle,
    Pending,
    Active,
}

/// Manages the "topic discovery" background process.
///
/// Peers can be interested in different topics within a single network. How topics are defined is up to
/// the applications. Multiple applications can even co-exist within the same network.
///
/// To find out which peer is interested in what topic we need a process called "topic discovery".
/// Currently this is (rather naively) implemented as a network-wide gossip overlay where peers
/// frequently broadcast their interests. Later we might look into other approaches, for example
/// applying a random-walk algorithm which traverses the network and learns about it over time.
// @TODO(adz): Would be great to already express this interface as traits so it's easier to swap
// out the strategies with something else. The API could even look similar to our current
// `Discovery` trait (for peer discovery), adjusted to work with topics.
pub struct TopicDiscovery {
    address_book: AddressBook,
    bootstrap: bool,
    gossip_actor_tx: mpsc::Sender<ToGossipActor>,
    network_id: NetworkId,
    status: Status,
}

impl TopicDiscovery {
    pub fn new(
        network_id: NetworkId,
        gossip_actor_tx: mpsc::Sender<ToGossipActor>,
        address_book: AddressBook,
        bootstrap: bool,
    ) -> Self {
        Self {
            address_book,
            bootstrap,
            gossip_actor_tx,
            network_id,
            status: Status::default(),
        }
    }

    /// Attempts joining the network-wide gossip overlay.
    pub async fn start(&mut self) -> Result<()> {
        // This method may be invoked before any peers have been discovered; in the case
        // of local discovery (mDNS), this will result in a downstream blockage when
        // attempting to join the network-wide gossip (see `src/engine/gossip.rs`).
        // As a temporary bug fix, we ignore the status check to allow this method to be called
        // repeatedly when not acting as a bootstrap node.
        if !self.bootstrap && self.status != Status::Idle {
            return Ok(());
        }

        let peers = self
            .address_book
            .random_set(self.network_id, JOIN_PEERS_SAMPLE_LEN)
            .await;

        if !peers.is_empty() || self.bootstrap {
            self.status = Status::Pending;
            self.gossip_actor_tx
                .send(ToGossipActor::Join {
                    topic_id: self.network_id,
                    peers,
                })
                .await?;
        }

        Ok(())
    }

    /// Reset the topic discovery status to idle.
    ///
    /// Resetting the status allows the network-wide gossip overlay to be rejoined after a loss of
    /// network connectivity.
    pub async fn reset_status(&mut self) {
        self.status = Status::Idle;
    }

    pub fn on_gossip_joined(&mut self) {
        if self.status == Status::Active {
            return;
        }

        if self.status == Status::Idle {
            panic!("can't set state to 'active' if joining was never attempted")
        }

        self.status = Status::Active;
    }

    pub async fn on_gossip_message(&mut self, bytes: &[u8]) -> Result<(Vec<[u8; 32]>, PublicKey)> {
        let topic_discovery_message =
            TopicDiscoveryMessage::from_bytes(bytes).context("decode topic discovery message")?;
        if !topic_discovery_message.verify() {
            bail!("invalid signature detected in topic discovery message");
        }

        let public_key = topic_discovery_message.public_key();
        for topic_id in &topic_discovery_message.topic_ids {
            self.address_book.add_topic_id(public_key, *topic_id).await;
        }
        Ok((topic_discovery_message.topic_ids, public_key))
    }

    pub async fn announce(&self, topic_ids: Vec<[u8; 32]>, private_key: &PrivateKey) -> Result<()> {
        if self.status != Status::Active {
            return Ok(());
        }

        let message = TopicDiscoveryMessage::new(topic_ids, private_key);

        self.gossip_actor_tx
            .send(ToGossipActor::Broadcast {
                topic_id: self.network_id,
                bytes: message.to_bytes(),
            })
            .await?;

        Ok(())
    }
}

type MessageId = [u8; 32];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TopicDiscoveryMessage {
    pub id: MessageId,
    pub topic_ids: Vec<[u8; 32]>,
    pub public_key: PublicKey,
    pub signature: Signature,
}

impl TopicDiscoveryMessage {
    pub fn new(topic_ids: Vec<[u8; 32]>, private_key: &PrivateKey) -> Self {
        // Message id is used to make every message unique, as duplicates get otherwise dropped
        // during gossip broadcast.
        let id = random();

        let public_key = private_key.public_key();
        let raw_message = (id, topic_ids.clone(), public_key);
        let signature = private_key.sign(&raw_message.to_bytes());

        Self {
            id,
            topic_ids,
            public_key,
            signature,
        }
    }

    pub fn verify(&self) -> bool {
        self.public_key.verify(
            &(self.id, &self.topic_ids, self.public_key).to_bytes(),
            &self.signature,
        )
    }

    pub fn public_key(&self) -> PublicKey {
        self.public_key
    }
}

#[cfg(test)]
mod tests {
    use p2panda_core::PrivateKey;
    use tokio::sync::mpsc;

    use crate::engine::AddressBook;
    use crate::{NodeAddress, bytes::ToBytes};

    use super::{Status, TopicDiscovery, TopicDiscoveryMessage};

    #[tokio::test]
    async fn ensure_status_reset() {
        let network_id = [7; 32];

        let mut address_book = AddressBook::new(network_id);
        let private_key = PrivateKey::new();
        let node_addr = NodeAddress::from_public_key(private_key.public_key());
        address_book.add_peer(node_addr).await;

        let (gossip_actor_tx, _gossip_actor_rx) = mpsc::channel(64);
        let mut topic_discovery =
            TopicDiscovery::new(network_id, gossip_actor_tx, address_book, true);

        // We expect the status to transition from `Idle` to `Pending` when topic discovery is
        // started, since we already added a peer to the address book.
        topic_discovery.start().await.unwrap();
        assert_eq!(topic_discovery.status, Status::Pending);

        // Status should revert to `Idle` after the reset.
        topic_discovery.reset_status().await;
        assert_eq!(topic_discovery.status, Status::Idle);
    }

    #[test]
    fn verify_message() {
        let private_key = PrivateKey::new();
        let topic_ids = vec![[0; 32]];
        let message = TopicDiscoveryMessage::new(topic_ids.clone(), &private_key);
        assert!(message.verify());

        let wrong_public_key = PrivateKey::new();
        let wrong_signature = wrong_public_key.sign(&topic_ids.to_bytes());
        let mut message = message;
        message.signature = wrong_signature;
        assert!(!message.verify())
    }
}
