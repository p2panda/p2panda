// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::Arc;

use anyhow::Result;
use futures_lite::future::Boxed as BoxedFuture;
use iroh_net::endpoint::{self, Connecting, Connection};
use p2panda_sync::traits::SyncProtocol;
use p2panda_sync::SyncError;
use tokio::sync::mpsc;
use tracing::{debug, debug_span};

use crate::engine::ToEngineActor;
use crate::protocols::ProtocolHandler;
use crate::sync;

pub const SYNC_CONNECTION_ALPN: &[u8] = b"/p2panda-net-sync/0";

#[allow(dead_code)]
#[derive(Debug)]
pub struct SyncConnection {
    sync_protocol: Arc<dyn for<'a> SyncProtocol<'a> + 'static>,
    engine_actor_tx: mpsc::Sender<ToEngineActor>,
}

impl SyncConnection {
    pub fn new(
        sync_protocol: Arc<dyn for<'a> SyncProtocol<'a> + 'static>,
        engine_actor_tx: mpsc::Sender<ToEngineActor>,
    ) -> Self {
        Self {
            sync_protocol,
            engine_actor_tx,
        }
    }

    // Handle an inbound connection using the `SYNC_CONNECTION_ALPN` and run a sync session with
    // the remote peer.
    async fn handle_connection(&self, connection: Connection) -> Result<()> {
        debug!("handling inbound sync connection...");
        let peer = endpoint::get_remote_node_id(&connection)?;
        let remote_addr = connection.remote_address();
        let connection_id = connection.stable_id() as u64;
        let _span = debug_span!("connection", connection_id, %remote_addr);

        // Create a bidirectional stream on the connection.
        let (mut send, mut recv) = connection
            .accept_bi()
            .await
            .map_err(|e| SyncError::Protocol(e.to_string()))?;

        let sync_protocol = self.sync_protocol.clone();
        let engine_actor_tx = self.engine_actor_tx.clone();

        // Run a sync session as the acceptor (aka. responder).
        let result =
            sync::accept_sync(&mut send, &mut recv, peer, sync_protocol, engine_actor_tx).await;

        // Clean-up the streams.
        send.finish()?;
        send.stopped().await?;
        recv.read_to_end(0).await?;

        if result.is_ok() {
            debug!("sync success: accept")
        } else {
            debug!("sync failure: accept")
        }

        Ok(())
    }
}

impl ProtocolHandler for SyncConnection {
    fn accept(self: Arc<Self>, connecting: Connecting) -> BoxedFuture<Result<()>> {
        Box::pin(async move { self.handle_connection(connecting.await?).await })
    }
}
