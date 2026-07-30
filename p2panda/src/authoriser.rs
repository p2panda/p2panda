// SPDX-License-Identifier: MIT OR Apache-2.0

//! Authoriser for maintaining and enforcing allow- and block-lists.
use std::collections::HashSet;
use std::fmt::Display;
use std::sync::Arc;

use p2panda_core::VerifyingKey;
use p2panda_net::iroh_endpoint::{BeforeConnectOutcome, EndpointAddr, EndpointHooks};
use p2panda_net::utils::{ShortFormat, to_verifying_key};
use tokio::sync::RwLock;
use tokio::sync::broadcast::{self, Receiver, Sender};
use tracing::warn;

/// Authoriser mode for determining how connections are accepted and rejected.
#[derive(Clone, Debug)]
enum AuthoriserMode {
    /// Allow all connections except for nodes which have been explicitly blocked.
    Permissive,
    /// Block all connections except for nodes which have been explicitly allowed.
    Restrictive,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthoriserEvent {
    Blocked(VerifyingKey),
}

impl Display for AuthoriserEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthoriserEvent::Blocked(node) => {
                write!(f, "blocked connection attempt to {}", node.fmt_short(),)
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct Authoriser {
    inner: Arc<RwLock<AuthoriserInner>>,
}

#[derive(Clone, Debug)]
pub struct AuthoriserInner {
    mode: AuthoriserMode,
    allowlist: HashSet<VerifyingKey>,
    blocklist: HashSet<VerifyingKey>,
    tx: Sender<AuthoriserEvent>,
}

impl Authoriser {
    /// Returns a connection authoriser and a receiver for authoriser events.
    ///
    /// Defaults to `restrictive` mode, meaning that nodes need to be explicitly added to the
    /// allowlist if connection attempts are to be accepted.
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel(128);

        let inner = AuthoriserInner {
            mode: AuthoriserMode::Restrictive,
            allowlist: HashSet::new(),
            blocklist: HashSet::new(),
            tx,
        };

        Self {
            inner: Arc::new(RwLock::new(inner)),
        }
    }

    pub async fn events(&self) -> Receiver<AuthoriserEvent> {
        let authoriser = self.inner.write().await;
        authoriser.tx.subscribe()
    }

    pub async fn permissive(&self) {
        let mut authoriser = self.inner.write().await;
        authoriser.mode = AuthoriserMode::Permissive;
    }

    pub async fn restrictive(&self) {
        let mut authoriser = self.inner.write().await;
        authoriser.mode = AuthoriserMode::Restrictive;
    }

    /// Allow connections to the given node.
    pub async fn allow(&self, node: VerifyingKey) {
        let mut authoriser = self.inner.write().await;
        authoriser.allowlist.insert(node);
    }

    /// Block connections to the given node.
    pub async fn block(&self, node: VerifyingKey) {
        let mut authoriser = self.inner.write().await;
        authoriser.blocklist.insert(node);
    }
}

impl Default for Authoriser {
    fn default() -> Self {
        Self::new()
    }
}

impl EndpointHooks for Authoriser {
    // Runs before an outgoing connection begins.
    async fn before_connect(
        &self,
        remote_addr: &EndpointAddr,
        _alpn: &[u8],
    ) -> BeforeConnectOutcome {
        let node = to_verifying_key(remote_addr.id);

        let authoriser = self.inner.read().await;

        match authoriser.mode {
            AuthoriserMode::Permissive => {
                if !authoriser.blocklist.contains(&node) {
                    return BeforeConnectOutcome::Accept;
                }
            }
            AuthoriserMode::Restrictive => {
                if authoriser.allowlist.contains(&node) {
                    return BeforeConnectOutcome::Accept;
                }
            }
        }

        if let Err(err) = authoriser.tx.send(AuthoriserEvent::Blocked(node)) {
            warn!("failed to send authoriser event: {}", err)
        }

        BeforeConnectOutcome::Reject
    }
}
