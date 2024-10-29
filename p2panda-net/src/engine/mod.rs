// SPDX-License-Identifier: AGPL-3.0-or-later

#[allow(clippy::module_inception)]
mod engine;
mod gossip;
mod message;

pub use engine::ToEngineActor;

use std::fmt::Debug;
use std::sync::Arc;

use anyhow::Result;
use futures_util::future::{MapErr, Shared};
use futures_util::{FutureExt, TryFutureExt};
use iroh_gossip::net::Gossip;
use iroh_net::{Endpoint, NodeAddr};
use p2panda_sync::{SyncProtocol, Topic};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinError;
use tokio_util::task::AbortOnDropHandle;
use tracing::{debug, error};

use crate::engine::engine::EngineActor;
use crate::engine::gossip::GossipActor;
use crate::network::{FromNetwork, JoinErrToStr, ToNetwork};
use crate::sync::manager::SyncManager;
use crate::sync::SyncConnection;
use crate::{NetworkId, TopicId};

/// The `Engine` is responsible for instantiating various system actors (including engine, gossip
/// and sync connection actors) and exposes an API for interacting with the engine actor.
#[derive(Debug)]
pub struct Engine<T> {
    engine_actor_tx: mpsc::Sender<ToEngineActor<T>>,
    sync_protocol: Option<Arc<dyn for<'a> SyncProtocol<'a, T> + 'static>>,
    #[allow(dead_code)]
    actor_handle: Shared<MapErr<AbortOnDropHandle<()>, JoinErrToStr>>,
}

impl<T> Engine<T>
where
    T: Topic + TopicId + 'static,
{
    pub fn new(
        network_id: NetworkId,
        endpoint: Endpoint,
        gossip: Gossip,
        sync_protocol: Option<Arc<dyn for<'a> SyncProtocol<'a, T> + 'static>>,
    ) -> Self {
        let (engine_actor_tx, engine_actor_rx) = mpsc::channel(64);
        let (gossip_actor_tx, gossip_actor_rx) = mpsc::channel(256);

        let (sync_actor, sync_actor_tx) = if let Some(ref sync_protocol) = sync_protocol {
            let (sync_actor, sync_actor_tx) = SyncManager::new(
                endpoint.clone(),
                engine_actor_tx.clone(),
                sync_protocol.clone(),
            );
            (Some(sync_actor), Some(sync_actor_tx))
        } else {
            (None, None)
        };

        let engine_actor = EngineActor::new(
            endpoint,
            engine_actor_rx,
            gossip_actor_tx,
            sync_actor_tx,
            network_id,
        );
        let gossip_actor = GossipActor::new(gossip_actor_rx, gossip, engine_actor_tx.clone());

        let actor_handle = tokio::task::spawn(async move {
            if let Err(err) = engine_actor.run(gossip_actor, sync_actor).await {
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

    /// Adds a peer to the address book.
    ///
    /// This method can be manually called to register known peers or automatically, for example by
    /// an background "peer discovery" process.
    ///
    /// Learning about a peer gives us information on how to connect to them, for learning about
    /// the topics it's interested in we need a separate process named "topic discovery".
    pub async fn add_peer(&self, node_addr: NodeAddr) -> Result<()> {
        self.engine_actor_tx
            .send(ToEngineActor::AddPeer { node_addr })
            .await?;
        Ok(())
    }

    /// Retrieves the node addresses of all peers the engine actor currently knows about.
    pub async fn known_peers(&self) -> Result<Vec<NodeAddr>> {
        let (reply, reply_rx) = oneshot::channel();
        self.engine_actor_tx
            .send(ToEngineActor::KnownPeers { reply })
            .await?;
        reply_rx.await?
    }

    /// Subscribes to the given topic and provides a channel for network message passing.
    pub async fn subscribe(
        &self,
        topic: T,
        from_network_tx: broadcast::Sender<FromNetwork>,
        to_network_rx: mpsc::Receiver<ToNetwork>,
        gossip_ready_tx: oneshot::Sender<()>,
    ) -> Result<()> {
        self.engine_actor_tx
            .send(ToEngineActor::Subscribe {
                topic,
                from_network_tx,
                to_network_rx,
                gossip_ready_tx,
            })
            .await?;
        Ok(())
    }

    /// Sends a shutdown signal to the engine actor and waits for a confirmation reply.
    pub async fn shutdown(&self) -> Result<()> {
        let (reply, reply_rx) = oneshot::channel();
        self.engine_actor_tx
            .send(ToEngineActor::Shutdown { reply })
            .await?;
        reply_rx.await?;
        debug!("engine shutdown");
        Ok(())
    }

    /// Returns a sync connection protocol handler for inbound connections.
    // @TODO: This method feels like the odd-one-out in this module. Could we move it somewhere
    // else?
    pub(super) fn sync_handler(&self) -> Option<SyncConnection<T>> {
        self.sync_protocol.as_ref().map(|sync_protocol| {
            SyncConnection::new(sync_protocol.clone(), self.engine_actor_tx.clone())
        })
    }
}
