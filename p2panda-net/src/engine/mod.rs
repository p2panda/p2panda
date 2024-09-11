// SPDX-License-Identifier: AGPL-3.0-or-later

#[allow(clippy::module_inception)]
mod engine;
mod gossip;
mod message;
pub mod sync;

pub use engine::ToEngineActor;

use std::sync::Arc;

use anyhow::Result;
use iroh_gossip::net::Gossip;
use iroh_net::util::SharedAbortingJoinHandle;
use iroh_net::{Endpoint, NodeAddr};
use p2panda_sync::traits::SyncProtocol;
use sync::SyncActor;
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing::{debug, error};

use crate::engine::engine::EngineActor;
use crate::engine::gossip::GossipActor;
use crate::network::{InEvent, OutEvent};
use crate::sync_connection::SyncConnection;
use crate::{NetworkId, TopicId};

#[derive(Debug)]
pub struct Engine {
    engine_actor_tx: mpsc::Sender<ToEngineActor>,
    #[allow(dead_code)]
    actor_handle: SharedAbortingJoinHandle<()>,
}

impl Engine {
    pub fn new(
        network_id: NetworkId,
        endpoint: Endpoint,
        gossip: Gossip,
        sync_protocol: Arc<dyn SyncProtocol + 'static>,
    ) -> Self {
        let (engine_actor_tx, engine_actor_rx) = mpsc::channel(64);
        let (gossip_actor_tx, gossip_actor_rx) = mpsc::channel(256);
        let (sync_actor_tx, sync_actor_rx) = mpsc::channel(256);

        let engine_actor = EngineActor::new(
            endpoint,
            gossip_actor_tx,
            sync_actor_tx,
            engine_actor_rx,
            network_id.into(),
        );
        let gossip_actor = GossipActor::new(gossip_actor_rx, gossip, engine_actor_tx.clone());
        let sync_actor = SyncActor::new(sync_actor_rx, sync_protocol, engine_actor_tx.clone());

        let actor_handle = tokio::task::spawn(async move {
            if let Err(err) = engine_actor.run(gossip_actor, sync_actor).await {
                error!("engine actor failed: {err:?}");
            }
        });

        Self {
            engine_actor_tx,
            actor_handle: actor_handle.into(),
        }
    }

    pub fn sync_handler(&self) -> SyncConnection {
        SyncConnection::new(self.engine_actor_tx.clone())
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
        debug!("engine shutdown");
        Ok(())
    }
}
