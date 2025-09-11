// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;

use p2panda_client::{Checkpoint, Query, QueryError};
use p2panda_core::{Hash, PrivateKey, PublicKey};
use p2panda_net::NetworkBuilder;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc;
use tokio_util::task::AbortOnDropHandle;

use crate::actor::{NodeActor, ToNodeActor};
use crate::config::Configuration;

pub struct Node {
    network_actor_tx: mpsc::Sender<ToNodeActor>,
    actor_drop_handle: AbortOnDropHandle<()>,
}

impl Node {
    pub async fn new(config: Configuration, private_key: PrivateKey) -> Result<Self, NodeError> {
        // @TODO: What is the role of the network id?
        let network = NetworkBuilder::new(config.network_id.into())
            .private_key(private_key)
            .build()
            .await?;

        // @TODO: Returned subscribe handle should implement unsubscribe on drop.
        // let (tx, rx, _ready) = network.subscribe(Hash::new(b"test").into()).await?;

        // Run actor in spawned task.

        let (network_actor_tx, network_actor_rx) = mpsc::channel(32);

        let actor_drop_handle = {
            let actor = NodeActor::new(network, network_actor_rx);

            let actor_handle = tokio::spawn(async {
                actor.run().await;
            });

            AbortOnDropHandle::new(actor_handle)
        };

        Ok(Self {
            network_actor_tx,
            actor_drop_handle,
        })
    }

    pub async fn publish(&self) {
        // @TODO
    }

    pub async fn subscribe(&self, query: Query, from: Checkpoint, live: bool) -> Subscription {
        // @TODO
        Subscription {
            id: 0,
            query,
            from,
            live,
        }
    }

    pub async fn subscribe_ephemeral(&self, topic_id: TopicId) -> EphemeralSubscription {
        // @TODO
        EphemeralSubscription { id: 0, topic_id }
    }
}

#[derive(Debug, Error)]
pub enum NodeError {
    // @TODO: No anyhow errors should come from p2panda crate.
    #[error(transparent)]
    Network(#[from] anyhow::Error),

    #[error(transparent)]
    Query(#[from] QueryError),
}

pub type TopicId = [u8; 32];

pub struct Subscription {
    id: usize,
    query: Query,
    from: Checkpoint,
    live: bool,
}

pub struct EphemeralSubscription {
    id: usize,
    topic_id: TopicId,
}

impl EphemeralSubscription {
    pub fn publish(&self) {
        // @TODO
    }
}
