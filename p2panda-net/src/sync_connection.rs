// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::Arc;

use anyhow::Result;
use futures_lite::future::Boxed as BoxedFuture;
use iroh_net::endpoint::{self, Connecting, Connection};
use tokio::sync::mpsc;
use tracing::debug_span;

use crate::engine::ToEngineActor;
use crate::protocols::ProtocolHandler;

pub const SYNC_CONNECTION_ALPN: &[u8] = b"/p2panda-net-sync/0";

#[allow(dead_code)]
#[derive(Debug)]
pub struct SyncConnection {
    engine_actor_tx: mpsc::Sender<ToEngineActor>,
}

impl SyncConnection {
    pub fn new(engine_actor_tx: mpsc::Sender<ToEngineActor>) -> Self {
        Self { engine_actor_tx }
    }

    async fn handle_connection(&self, connection: Connection) -> Result<()> {
        let peer = endpoint::get_remote_node_id(&connection)?;
        let remote_addr = connection.remote_address();
        let connection_id = connection.stable_id() as u64;
        let _span = debug_span!("connection", connection_id, %remote_addr);

        self.engine_actor_tx
            .send(ToEngineActor::AcceptSync { peer, connection })
            .await?;

        Ok(())
    }
}

impl ProtocolHandler for SyncConnection {
    fn accept(self: Arc<Self>, connecting: Connecting) -> BoxedFuture<Result<()>> {
        Box::pin(async move { self.handle_connection(connecting.await?).await })
    }
}
