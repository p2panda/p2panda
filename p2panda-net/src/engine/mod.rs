// SPDX-License-Identifier: AGPL-3.0-or-later

#[allow(clippy::module_inception)]
mod engine;
mod gossip;
mod message;

use anyhow::Result;
use iroh_gossip::net::Gossip;
use iroh_net::util::SharedAbortingJoinHandle;
use iroh_net::{Endpoint, NodeAddr};
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing::error;

use crate::engine::engine::{EngineActor, ToEngineActor};
use crate::engine::gossip::GossipActor;
use crate::network::{InEvent, OutEvent};
use crate::{NetworkId, TopicId};

#[derive(Debug)]
pub struct Engine {
    engine_actor_tx: mpsc::Sender<ToEngineActor>,
    #[allow(dead_code)]
    actor_handle: SharedAbortingJoinHandle<()>,
}

impl Engine {
    pub fn new(network_id: NetworkId, endpoint: Endpoint, gossip: Gossip) -> Self {
        let (engine_actor_tx, engine_actor_rx) = mpsc::channel(64);
        let (gossip_actor_tx, gossip_actor_rx) = mpsc::channel(256);

        let engine_actor = EngineActor::new(
            endpoint,
            gossip.clone(),
            gossip_actor_tx,
            engine_actor_rx,
            network_id.into(),
        );
        let gossip_actor = GossipActor::new(gossip_actor_rx, gossip, engine_actor_tx.clone());

        let actor_handle = tokio::task::spawn(async move {
            if let Err(err) = engine_actor.run(gossip_actor).await {
                error!("engine actor failed: {err:?}");
            }
        });

        Self {
            engine_actor_tx,
            actor_handle: actor_handle.into(),
        }
    }

    pub async fn add_peer(&self, node_addr: NodeAddr) -> Result<()> {
        self.engine_actor_tx
            .send(ToEngineActor::AddPeer { node_addr })
            .await?;
        Ok(())
    }

    pub async fn known_peers(&self) -> Result<Vec<NodeAddr>> {
        let (reply, reply_rx) = oneshot::channel();
        self.engine_actor_tx
            .send(ToEngineActor::KnownPeers { reply })
            .await?;
        reply_rx.await?
    }

    pub async fn subscribe(
        &self,
        topic: TopicId,
        out_tx: broadcast::Sender<OutEvent>,
        in_rx: mpsc::Receiver<InEvent>,
    ) -> Result<()> {
        self.engine_actor_tx
            .send(ToEngineActor::Subscribe {
                topic: topic.into(),
                out_tx,
                in_rx,
            })
            .await?;
        Ok(())
    }

    pub async fn shutdown(&self) -> Result<()> {
        let (reply, reply_rx) = oneshot::channel();
        self.engine_actor_tx
            .send(ToEngineActor::Shutdown { reply })
            .await?;
        reply_rx.await?;
        Ok(())
    }
}
