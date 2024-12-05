// SPDX-License-Identifier: MIT OR Apache-2.0

use anyhow::{bail, Context, Result};
use iroh_net::key::{PublicKey, SecretKey, Signature};
use rand::random;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::bytes::{FromBytes, ToBytes};
use crate::engine::address_book::AddressBook;
use crate::engine::constants::JOIN_PEERS_SAMPLE_LEN;
use crate::engine::gossip::ToGossipActor;
use crate::NetworkId;

#[derive(Default, PartialEq, Eq)]
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
    gossip_actor_tx: mpsc::Sender<ToGossipActor>,
    network_id: NetworkId,
    status: Status,
}

impl TopicDiscovery {
    pub fn new(
        network_id: NetworkId,
        gossip_actor_tx: mpsc::Sender<ToGossipActor>,
        address_book: AddressBook,
    ) -> Self {
        Self {
            address_book,
            gossip_actor_tx,
            network_id,
            status: Status::default(),
        }
    }

    /// Attempts joining the network-wide gossip overlay.
    pub async fn start(&mut self) -> Result<()> {
        if self.status != Status::Idle {
            return Ok(());
        }

        let peers = self
            .address_book
            .random_set(self.network_id, JOIN_PEERS_SAMPLE_LEN)
            .await;

        if !peers.is_empty() {
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

    pub async fn announce(&self, topic_ids: Vec<[u8; 32]>, secret_key: &SecretKey) -> Result<()> {
        if self.status != Status::Active {
            return Ok(());
        }

        let message = TopicDiscoveryMessage::new(topic_ids, secret_key);

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
    pub fn new(topic_ids: Vec<[u8; 32]>, secret_key: &SecretKey) -> Self {
        // Message id is used to make every message unique, as duplicates get otherwise dropped
        // during gossip broadcast.
        let id = random();

        let public_key = secret_key.public();
        let raw_message = (id, topic_ids.clone(), public_key);
        let signature = secret_key.sign(&raw_message.to_bytes());

        Self {
            id,
            topic_ids,
            public_key,
            signature,
        }
    }

    pub fn verify(&self) -> bool {
        self.public_key
            .verify(
                &(self.id, &self.topic_ids, self.public_key).to_bytes(),
                &self.signature,
            )
            .is_ok()
    }

    pub fn public_key(&self) -> PublicKey {
        self.public_key
    }
}

#[cfg(test)]
mod tests {
    use iroh_net::key::SecretKey;

    use crate::bytes::ToBytes;

    use super::TopicDiscoveryMessage;

    #[test]
    fn verify_message() {
        let secret_key = SecretKey::generate();
        let topic_ids = vec![[0; 32]];
        let message = TopicDiscoveryMessage::new(topic_ids.clone(), &secret_key);
        assert!(message.verify());

        let wrong_secret_key = SecretKey::generate();
        let wrong_signature = wrong_secret_key.sign(&topic_ids.to_bytes());
        let mut message = message;
        message.signature = wrong_signature;
        assert!(!message.verify())
    }
}
