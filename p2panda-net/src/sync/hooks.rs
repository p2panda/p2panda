// SPDX-License-Identifier: MIT OR Apache-2.0

//! Hooks to intercept incoming and outgoing sync sessions.
use std::pin::Pin;

use crate::NodeId;

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Outcome of [`SyncHooks::after_handshake`]
#[derive(Debug)]
pub enum AfterHandshakeOutcome {
    /// Accept the sync attempt.
    Accept,

    /// Reject the sync attempt.
    Reject,
}

/// Sync hooks intercept the sync session establishment process.
pub trait SyncHooks: std::fmt::Debug + Send + Sync {
    /// Payload which was delivered during sync session handshake.
    ///
    /// Usually this contains what is being synced, like a topic.
    type Handshake: std::fmt::Debug + Send + Sync;

    /// Intercept outgoing or incoming sync session before it is started and actual application data
    /// is exchanged.
    fn after_handshake<'a>(
        &'a self,
        _remote_node_id: NodeId,
        _handshake: &'a Self::Handshake,
    ) -> impl Future<Output = AfterHandshakeOutcome> + Send + 'a {
        async { AfterHandshakeOutcome::Accept }
    }
}

pub trait DynSyncHooks<H>: std::fmt::Debug + Send + Sync {
    fn after_handshake<'a>(
        &'a self,
        remote_node_id: NodeId,
        handshake: &'a H,
    ) -> BoxFuture<'a, AfterHandshakeOutcome>;

    fn clone_dyn(&self) -> Box<dyn DynSyncHooks<H>>;
}

impl<T: SyncHooks + Clone + 'static> DynSyncHooks<T::Handshake> for T {
    fn after_handshake<'a>(
        &'a self,
        remote_node_id: NodeId,
        handshake: &'a T::Handshake,
    ) -> BoxFuture<'a, AfterHandshakeOutcome> {
        Box::pin(SyncHooks::after_handshake(self, remote_node_id, handshake))
    }

    fn clone_dyn(&self) -> Box<dyn DynSyncHooks<T::Handshake>> {
        Box::new(self.clone())
    }
}

impl<H> Clone for Box<dyn DynSyncHooks<H>> {
    fn clone(&self) -> Self {
        self.clone_dyn()
    }
}

#[derive(Debug, Default, Clone)]
pub struct SyncHooksList<H> {
    inner: Vec<Box<dyn DynSyncHooks<H>>>,
}

impl<H> SyncHooksList<H>
where
    H: std::fmt::Debug + Send + Sync,
{
    pub fn new() -> Self {
        Self { inner: Vec::new() }
    }

    pub fn push(&mut self, hook: impl SyncHooks<Handshake = H> + 'static + Clone) {
        let hook = Box::new(hook);
        self.inner.push(hook);
    }
}

impl<H> SyncHooks for SyncHooksList<H>
where
    H: std::fmt::Debug + Send + Sync,
{
    type Handshake = H;

    async fn after_handshake(
        &self,
        remote_node_id: NodeId,
        handshake: &H,
    ) -> AfterHandshakeOutcome {
        for hook in self.inner.iter() {
            match hook.after_handshake(remote_node_id, handshake).await {
                AfterHandshakeOutcome::Accept => continue,
                reject @ AfterHandshakeOutcome::Reject => {
                    return reject;
                }
            }
        }
        AfterHandshakeOutcome::Accept
    }
}
