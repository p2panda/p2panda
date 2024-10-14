// SPDX-License-Identifier: AGPL-3.0-or-later

#[allow(clippy::module_inception)]
mod engine;
mod gossip;
mod message;

pub use engine::ToEngineActor;

use std::sync::Arc;

use anyhow::Result;
use futures_util::future::{MapErr, Shared};
use futures_util::{FutureExt, TryFutureExt};
use iroh_gossip::net::Gossip;
use iroh_net::{Endpoint, NodeAddr};
use p2panda_sync::traits::SyncProtocol;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinError;
use tokio_util::task::AbortOnDropHandle;
use tracing::{debug, error};

use crate::engine::engine::EngineActor;
use crate::engine::gossip::GossipActor;
use crate::network::{FromNetwork, ToNetwork};
use crate::sync::{SyncConnection, SyncManager};
use crate::{JoinErrToStr, NetworkId, TopicId};

#[derive(Debug)]
pub struct Engine {
    engine_actor_tx: mpsc::Sender<ToEngineActor>,
    sync_protocol: Option<Arc<dyn for<'a> SyncProtocol<'a> + 'static>>,
    #[allow(dead_code)]
    actor_handle: Shared<MapErr<AbortOnDropHandle<()>, JoinErrToStr>>,
}

impl Engine {
    pub fn new(
        network_id: NetworkId,
        endpoint: Endpoint,
        gossip: Gossip,
        sync_protocol: Option<Arc<dyn for<'a> SyncProtocol<'a> + 'static>>,
    ) -> Self {
        let (engine_actor_tx, engine_actor_rx) = mpsc::channel(64);
        let (gossip_actor_tx, gossip_actor_rx) = mpsc::channel(256);

        // Create a sync manager if a sync protocol has been provided.
        let sync_manager = sync_protocol.as_ref().map(|sync_protocol| {
            SyncManager::new(
                endpoint.clone(),
                engine_actor_tx.clone(),
                sync_protocol.clone(),
            )
        });

        let engine_actor = EngineActor::new(
            endpoint,
            gossip_actor_tx,
            sync_manager,
            engine_actor_rx,
            network_id.into(),
        );
        let gossip_actor = GossipActor::new(gossip_actor_rx, gossip, engine_actor_tx.clone());

        let actor_handle = tokio::task::spawn(async move {
            if let Err(err) = engine_actor.run(gossip_actor).await {
                error!("engine actor failed: {err:?}");
            }
        });

        let actor_drop_handle = AbortOnDropHandle::new(actor_handle)
            .map_err(Box::new(|e: JoinError| e.to_string()) as JoinErrToStr)
            .shared();

        Self {
            engine_actor_tx,
            actor_handle: actor_drop_handle,
            sync_protocol,
        }
    }

    pub fn sync_handler(&self) -> Option<SyncConnection> {
        self.sync_protocol.as_ref().map(|sync_protocol| {
            SyncConnection::new(sync_protocol.clone(), self.engine_actor_tx.clone())
        })
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
        from_network_tx: broadcast::Sender<FromNetwork>,
        to_network_rx: mpsc::Receiver<ToNetwork>,
    ) -> Result<()> {
        self.engine_actor_tx
            .send(ToEngineActor::Subscribe {
                topic: topic.into(),
                from_network_tx,
                to_network_rx,
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
