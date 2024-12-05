// SPDX-License-Identifier: AGPL-3.0-or-later

//! Blobs protocol handler implementation for accepting inbound network connections.
use std::sync::Arc;

use anyhow::Result;
use futures_lite::future::Boxed as BoxedFuture;
use iroh_blobs::protocol::ALPN;
use iroh_blobs::provider::{self, EventSender};
use iroh_blobs::store::Store;
use iroh_blobs::util::local_pool::LocalPoolHandle;
use iroh_net::endpoint::Connecting;
use p2panda_net::ProtocolHandler;

/// Application-Layer Protocol Negotiation (ALPN) identifier for blobs.
pub const BLOBS_ALPN: &[u8] = ALPN;

/// Blobs connection handler.
#[derive(Debug)]
pub struct BlobsProtocol<S> {
    rt: LocalPoolHandle,
    store: S,
}

impl<S: Store> BlobsProtocol<S> {
    /// Returns a new instance of `BlobsProtocol` using the given store and local task pool.
    ///
    /// `BlobsProtocol` implements the `ProtocolHandler` trait, allowing it to accept inbound
    /// network connections for the purposes of blob sync.
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
