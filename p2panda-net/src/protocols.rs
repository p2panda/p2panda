// SPDX-License-Identifier: MIT OR Apache-2.0

use std::any::Any;
use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use anyhow::Result;
use futures_lite::future::Boxed as BoxedFuture;
use futures_util::future::join_all;
use iroh::endpoint::Connecting;
use tracing::debug;

/// Interface to accept incoming connections for custom protocol implementations.
///
/// A node can accept connections for custom protocols. By default, the node only accepts
/// connections for the core protocols (gossip and optionally sync or blobs).
pub trait ProtocolHandler: Send + Sync + IntoArcAny + fmt::Debug + 'static {
    /// Handle an incoming connection.
    ///
    /// This runs on a freshly spawned tokio task so this can be long-running.
    fn accept(self: Arc<Self>, conn: Connecting) -> BoxedFuture<Result<()>>;

    /// Called when the node shuts down.
    fn shutdown(self: Arc<Self>) -> BoxedFuture<()> {
        Box::pin(async move {})
    }
}

/// Helper trait to facilitate casting from `Arc<dyn T>` to `Arc<dyn Any>`.
///
/// This trait has a blanket implementation so there is no need to implement this yourself.
pub trait IntoArcAny {
    fn into_arc_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync>;
}

impl<T: Send + Sync + 'static> IntoArcAny for T {
    fn into_arc_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct ProtocolMap(BTreeMap<&'static [u8], Arc<dyn ProtocolHandler>>);

impl ProtocolMap {
    /// Returns the registered protocol handler for an ALPN as a [`Arc<dyn ProtocolHandler>`].
    pub(super) fn get(&self, alpn: &[u8]) -> Option<Arc<dyn ProtocolHandler>> {
        self.0.get(alpn).cloned()
    }

    /// Inserts a protocol handler.
    pub(super) fn insert(&mut self, alpn: &'static [u8], handler: Arc<dyn ProtocolHandler>) {
        self.0.insert(alpn, handler);
    }

    /// Returns an iterator of all registered ALPN protocol identifiers.
    pub(super) fn alpns(&self) -> Vec<Vec<u8>> {
        self.0.keys().map(|alpn| alpn.to_vec()).collect::<Vec<_>>()
    }

    /// Shuts down all protocol handlers.
    ///
    /// Calls and awaits [`ProtocolHandler::shutdown`] for all registered handlers concurrently.
    pub(super) async fn shutdown(&self) {
        let handlers = self.0.values().cloned().map(ProtocolHandler::shutdown);
        debug!("await all handler shutdown handles");
        join_all(handlers).await;
        debug!("all handlers closed");
    }
}

impl ProtocolHandler for iroh_gossip::net::Gossip {
    fn accept(self: Arc<Self>, conn: Connecting) -> BoxedFuture<Result<()>> {
        Box::pin(async move {
            self.handle_connection(conn.await?).await?;
            Ok(())
        })
    }
}
