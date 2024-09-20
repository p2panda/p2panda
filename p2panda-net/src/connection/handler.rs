// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::Arc;

use anyhow::Result;
use futures_lite::future::Boxed as BoxedFuture;
use iroh_net::endpoint::{self, Connecting, Connection};
use tokio::sync::mpsc;
use tracing::{debug, debug_span};

use crate::connection::ToConnectionActor;
use crate::protocols::ProtocolHandler;

pub const SYNC_CONNECTION_ALPN: &[u8] = b"/p2panda-net-sync/0";

#[allow(dead_code)]
#[derive(Debug)]
pub struct SyncConnection {
    connection_actor_tx: mpsc::Sender<ToConnectionActor>,
}

impl SyncConnection {
    pub fn new(connection_actor_tx: mpsc::Sender<ToConnectionActor>) -> Self {
        Self {
            connection_actor_tx,
        }
    }

    async fn handle_connection(&self, connection: Connection) -> Result<()> {
        debug!("handling inbound sync connection!");

        let peer = endpoint::get_remote_node_id(&connection)?;
        let remote_addr = connection.remote_address();
        let connection_id = connection.stable_id() as u64;
        let _span = debug_span!("connection", connection_id, %remote_addr);

        self.connection_actor_tx
            .send(ToConnectionActor::Connected { peer, connection })
            .await?;

        Ok(())
    }
}

impl ProtocolHandler for SyncConnection {
    fn accept(self: Arc<Self>, connecting: Connecting) -> BoxedFuture<Result<()>> {
        Box::pin(async move { self.handle_connection(connecting.await?).await })
    }
}
