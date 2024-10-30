// SPDX-License-Identifier: AGPL-3.0-or-later

use std::time::Duration;

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

    pub fn on_joined(&mut self) {
        if self.status == Status::Active {
            return;
        }

        if self.status == Status::Idle {
            panic!("can't set state to 'active' if joining was never attempted")
        }

        self.status = Status::Active;
    }

    pub async fn on_message(&mut self, bytes: &[u8], node_id: NodeId) -> Result<Vec<[u8; 32]>> {
        let topic_ids = TopicDiscoveryMessage::from_bytes(&bytes).map(|message| message.1)?;
        for topic_id in &topic_ids {
            self.address_book.add_topic(node_id, *topic_id).await;
        }
        Ok(topic_ids)
    }

    pub async fn announce(&self) -> Result<()> {
        if self.status != Status::Active {
            return Ok(());
        }

        // @TODO
        // let topics = self.address_book.earmarked().await;
        let message = TopicDiscoveryMessage::new(vec![]);

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
