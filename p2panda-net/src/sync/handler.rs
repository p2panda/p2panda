// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use futures_lite::future::Boxed as BoxedFuture;
use iroh::endpoint::{Connecting, Connection};
use p2panda_sync::{SyncProtocol, TopicQuery};
use tokio::sync::mpsc;
use tracing::{debug, debug_span};

use crate::engine::ToEngineActor;
use crate::protocols::ProtocolHandler;
use crate::{sync, to_public_key};

pub const SYNC_CONNECTION_ALPN: &[u8] = b"/p2panda-net-sync/0";

#[derive(Debug)]
pub struct SyncConnection<T> {
    sync_protocol: Arc<dyn for<'a> SyncProtocol<'a, T> + 'static>,
    engine_actor_tx: mpsc::Sender<ToEngineActor<T>>,
}

impl<T> SyncConnection<T>
where
    T: TopicQuery + 'static,
{
    pub fn new(
        sync_protocol: Arc<dyn for<'a> SyncProtocol<'a, T> + 'static>,
        engine_actor_tx: mpsc::Sender<ToEngineActor<T>>,
    ) -> Self {
        Self {
            sync_protocol,
            engine_actor_tx,
        }
    }

    /// Handle an inbound connection using the `SYNC_CONNECTION_ALPN` and accept a sync session.
    async fn handle_connection(&self, connection: Connection) -> Result<()> {
        let peer = to_public_key(connection.remote_node_id()?);
        let connection_id = connection.stable_id() as u64;

        let _span = debug_span!("connection", connection_id);
        debug!(parent: &_span, "handling inbound sync connection...");

        let (mut send, mut recv) = connection.accept_bi().await?;

        let sync_protocol = self.sync_protocol.clone();
        let engine_actor_tx = self.engine_actor_tx.clone();

        // Run a sync session as the "acceptor" (aka. "responder").
        //
        // Sync failure or successful completion is reported to the engine actor internally, so
        // there's no need for us to do that in the context of handling the connection.
        let result =
            sync::accept_sync(&mut send, &mut recv, peer, sync_protocol, engine_actor_tx).await;

        send.finish()?;
        send.stopped().await?;

        // This will error if there's been remaining bytes in the buffer, indicating that the
        // protocol was not followed as expected.
        recv.read_to_end(0).await?;

        if result.is_ok() {
            debug!(parent: &_span, "sync success as acceptor")
        }

        Ok(())
    }
}

impl<T> ProtocolHandler for SyncConnection<T>
where
    T: TopicQuery + 'static,
{
    fn accept(self: Arc<Self>, connecting: Connecting) -> BoxedFuture<Result<()>> {
        Box::pin(async move { self.handle_connection(connecting.await?).await })
    }
}
