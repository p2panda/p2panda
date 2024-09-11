// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::Arc;

use anyhow::Result;
use futures_lite::future::Boxed as BoxedFuture;
use iroh_net::endpoint::{self, Connecting, Connection};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, debug_span};

use crate::engine::ToEngineActor;
use crate::protocols::ProtocolHandler;

pub const SYNC_CONNECTION_ALPN: &[u8] = b"/p2panda-net-sync/";

#[allow(dead_code)]
#[derive(Debug)]
pub struct SyncConnection {
    engine_actor_tx: mpsc::Sender<ToEngineActor>,
}

impl SyncConnection {
    pub fn new(engine_actor_tx: mpsc::Sender<ToEngineActor>) -> Self {
        Self { engine_actor_tx }
    }

    async fn handle_connection(&self, alpn: Vec<u8>, connection: Connection) -> Result<()> {
        debug!("handling connection for alpn: {alpn:?}");
        let remote_addr = connection.remote_address();
        let connection_id = connection.stable_id() as u64;
        let _span = debug_span!("connection", connection_id, %remote_addr);

        let (send, recv) = connection.accept_bi().await?;
        let peer = endpoint::get_remote_node_id(&connection)?;
        debug!("bi-directional stream established with {}", peer);

        let (result_tx, result_rx) = oneshot::channel();

        self.engine_actor_tx
            .send(ToEngineActor::AcceptSync {
                peer,
                send,
                recv,
                result_tx,
            })
            .await?;

        result_rx.await?
    }
}

impl ProtocolHandler for SyncConnection {
    fn accept(self: Arc<Self>, mut connecting: Connecting) -> BoxedFuture<Result<()>> {
        debug!("received accept in protocol handler");
        Box::pin(async move {
            let alpn = connecting.alpn().await?;
            self.handle_connection(alpn, connecting.await?).await
        })
    }
}
