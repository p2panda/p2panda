// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use futures_lite::future::Boxed as BoxedFuture;
use iroh_blobs::protocol::ALPN;
use iroh_blobs::provider::{self, EventSender};
use iroh_blobs::store::Store;
use iroh_blobs::util::local_pool::LocalPoolHandle;
use iroh_net::endpoint::Connecting;
use p2panda_net::ProtocolHandler;

pub const BLOBS_ALPN: &[u8] = ALPN;

#[derive(Debug)]
pub struct BlobsProtocol<S> {
    rt: LocalPoolHandle,
    store: S,
}

impl<S: Store> BlobsProtocol<S> {
    pub fn new(store: S, rt: LocalPoolHandle) -> Self {
        Self { rt, store }
    }
}

impl<S: Store> ProtocolHandler for BlobsProtocol<S> {
    fn accept(self: Arc<Self>, conn: Connecting) -> BoxedFuture<Result<()>> {
        Box::pin(async move {
            provider::handle_connection(
                conn.await?,
                self.store.clone(),
                EventSender::default(),
                self.rt.clone(),
            )
            .await;
            Ok(())
        })
    }
}
