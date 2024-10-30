// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::Result;
use iroh_net::NodeId;
use rand::random;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::engine::address_book::AddressBook;
use crate::engine::constants::JOIN_PEERS_SAMPLE_LEN;
use crate::engine::gossip::ToGossipActor;
use crate::message::{FromBytes, ToBytes};
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
/// Within a network peers can be interested in different topics. How topics are defined is up to
/// the applications. Within a network there can be even multiple applications co-existing.
///
/// To find out which peer is interested in what topic we need a process called "topic discovery".
/// Currently this is (rather naively) implemented as a network-wide gossip overlay where peers
/// frequently broadcast their interests. Later we might look into other approaches, for example
/// applying a random-walk algorithm which traverses the network and learning about it over time.
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

    pub async fn on_gossip_message(
        &mut self,
        bytes: &[u8],
        node_id: NodeId,
    ) -> Result<Vec<[u8; 32]>> {
        let topic_ids = TopicDiscoveryMessage::from_bytes(bytes).map(|message| message.1)?;
        for topic_id in &topic_ids {
            self.address_book.add_topic_id(node_id, *topic_id).await;
        }
        Ok(topic_ids)
    }

    pub async fn announce(&self, topic_ids: Vec<[u8; 32]>) -> Result<()> {
        if self.status != Status::Active {
            return Ok(());
        }

        let message = TopicDiscoveryMessage::new(topic_ids);

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
pub struct TopicDiscoveryMessage(MessageId, pub Vec<[u8; 32]>);

impl TopicDiscoveryMessage {
    pub fn new(topic_ids: Vec<[u8; 32]>) -> Self {
        // Message id is used to make every message unique, as duplicates get otherwise dropped
        // during gossip broadcast.
        Self(random(), topic_ids)
    }
}
