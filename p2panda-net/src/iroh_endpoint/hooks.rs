// SPDX-License-Identifier: MIT OR Apache-2.0

//! Endpoint hooks traits and types.
use std::pin::Pin;

use iroh::EndpointAddr;
use iroh::endpoint::{AfterHandshakeOutcome, BeforeConnectOutcome, Connection, EndpointHooks};

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait DynEndpointHooks: std::fmt::Debug + Send + Sync {
    fn before_connect<'a>(
        &'a self,
        remote_addr: &'a EndpointAddr,
        alpn: &'a [u8],
    ) -> BoxFuture<'a, BeforeConnectOutcome>;

    fn after_handshake<'a>(&'a self, conn: &'a Connection) -> BoxFuture<'a, AfterHandshakeOutcome>;

    fn clone_dyn(&self) -> Box<dyn DynEndpointHooks>;
}

impl<T: EndpointHooks + Clone + 'static> DynEndpointHooks for T {
    fn before_connect<'a>(
        &'a self,
        remote_addr: &'a EndpointAddr,
        alpn: &'a [u8],
    ) -> BoxFuture<'a, BeforeConnectOutcome> {
        Box::pin(EndpointHooks::before_connect(self, remote_addr, alpn))
    }

    fn after_handshake<'a>(&'a self, conn: &'a Connection) -> BoxFuture<'a, AfterHandshakeOutcome> {
        Box::pin(EndpointHooks::after_handshake(self, conn))
    }

    fn clone_dyn(&self) -> Box<dyn DynEndpointHooks> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn DynEndpointHooks> {
    fn clone(&self) -> Self {
        self.clone_dyn()
    }
}

#[derive(Debug, Default, Clone)]
pub struct EndpointHooksList {
    pub inner: Vec<Box<dyn DynEndpointHooks>>,
}

impl EndpointHooksList {
    pub(crate) fn new() -> Self {
        Self { inner: Vec::new() }
    }

    pub(crate) fn push(&mut self, hook: impl EndpointHooks + 'static + Clone) {
        let hook = Box::new(hook);
        self.inner.push(hook);
    }
}

impl EndpointHooks for EndpointHooksList {
    async fn before_connect(
        &self,
        remote_addr: &EndpointAddr,
        alpn: &[u8],
    ) -> BeforeConnectOutcome {
        for hook in self.inner.iter() {
            match hook.before_connect(remote_addr, alpn).await {
                BeforeConnectOutcome::Accept => continue,
                reject @ BeforeConnectOutcome::Reject => {
                    return reject;
                }
            }
        }
        BeforeConnectOutcome::Accept
    }

    async fn after_handshake(&self, conn: &Connection) -> AfterHandshakeOutcome {
        for hook in self.inner.iter() {
            match hook.after_handshake(conn).await {
                AfterHandshakeOutcome::Accept => continue,
                reject @ AfterHandshakeOutcome::Reject { .. } => {
                    return reject;
                }
            }
        }
        AfterHandshakeOutcome::Accept
    }
}
