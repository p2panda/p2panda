// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::Arc;

use anyhow::Result;
use futures_lite::future::Boxed as BoxedFuture;
use iroh_gossip::proto::TopicId;
use iroh_net::endpoint::{self, Connecting, Connection};
use iroh_net::Endpoint;
use tokio::sync::{mpsc, oneshot};
use tracing::debug_span;

use crate::engine::sync::ToSyncActor;
use crate::protocols::ProtocolHandler;

pub const SYNC_CONNECTION_ALPN: &[u8] = b"/p2panda-net-sync/";

#[allow(dead_code)]
#[derive(Debug)]
pub struct SyncConnection {
    sync_actor_tx: mpsc::Sender<ToSyncActor>,
}

impl SyncConnection {
    pub fn new(sync_actor_tx: mpsc::Sender<ToSyncActor>) -> Self {
        Self { sync_actor_tx }
    }

    async fn handle_connection(&self, alpn: Vec<u8>, connection: Connection) -> Result<()> {
        let remote_addr = connection.remote_address();
        let connection_id = connection.stable_id() as u64;
        let _span = debug_span!("connection", connection_id, %remote_addr);

        let (mut send, mut recv) = connection.accept_bi().await?;

        // Extract the topic identifier from the ALPN.
        let mut topic = [0; 32];
        topic.copy_from_slice(&alpn[SYNC_CONNECTION_ALPN.len() + 1..]);

        let peer = endpoint::get_remote_node_id(&connection)?;

        let (result_tx, result_rx) = oneshot::channel();

        // ToSyncActor::SyncInitiate
        // ToSyncActor::SyncReceive
        //  - doesn't know topic yet; is sent in sync protocol by initiator

        self.sync_actor_tx
            .send(ToSyncActor::Sync {
                peer,
                topic: topic.into(),
                send,
                recv,
                result_tx,
            })
            .await?;

        result_rx.await?;

        Ok(())
    }
}

impl ProtocolHandler for SyncConnection {
    fn accept(self: Arc<Self>, mut connecting: Connecting) -> BoxedFuture<Result<()>> {
        Box::pin(async move {
            let alpn = connecting.alpn().await?;
            self.handle_connection(alpn, connecting.await?).await
        })
    }
}
