// SPDX-License-Identifier: MIT OR Apache-2.0

//! Manage lists to allow and block iroh connections.
use std::collections::HashSet;
use std::fmt::Display;
use std::sync::Arc;

use iroh::endpoint::Side;
use p2panda_core::traits::ShortFormat;
use tokio::sync::RwLock;
use tokio::sync::broadcast;
use tracing::warn;

use crate::NodeId;
use crate::iroh_endpoint::{
    AfterHandshakeOutcome, BeforeConnectOutcome, EndpointAddr, EndpointHooks,
};
use crate::utils::to_verifying_key;

/// Determines if the block/allow list is permissive or restrictive.
#[derive(Clone, Debug)]
pub enum BlockListMode {
    /// Allow all connections except for nodes which have been explicitly blocked.
    Permissive,

    /// Block all connections except for nodes which have been explicitly allowed.
    Restrictive,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectionRole {
    Acceptor,
    Initiator,
}

impl Display for ConnectionRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionRole::Acceptor => write!(f, "inbound"),
            ConnectionRole::Initiator => write!(f, "outbound"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectionBlockListEvent {
    Blocked { node: NodeId, role: ConnectionRole },
    Allowed { node: NodeId, role: ConnectionRole },
}

impl Display for ConnectionBlockListEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionBlockListEvent::Blocked { node, role } => {
                write!(
                    f,
                    "blocked {} connection attempt to {}",
                    role,
                    node.fmt_short(),
                )
            }
            ConnectionBlockListEvent::Allowed { node, role } => {
                write!(
                    f,
                    "allowed {} connection attempt to {}",
                    role,
                    node.fmt_short(),
                )
            }
        }
    }
}

/// Accept or reject iroh connections with a managed allow/block list.
#[derive(Clone, Debug)]
pub struct ConnectionBlockList {
    inner: Arc<RwLock<Inner>>,
}

#[derive(Debug)]
struct Inner {
    mode: BlockListMode,
    allow: HashSet<NodeId>,
    block: HashSet<NodeId>,
    tx: broadcast::Sender<ConnectionBlockListEvent>,
    rx: Option<broadcast::Receiver<ConnectionBlockListEvent>>,
}

impl Default for ConnectionBlockList {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectionBlockList {
    /// Returns allow/block list to authorise connection attempts.
    ///
    /// Defaults to `permissive` mode, meaning that connection attempts from all nodes which are not
    /// explicitly blocked will be accepted.
    pub fn new() -> Self {
        Self::with_mode(BlockListMode::Permissive)
    }

    /// Returns allow/block list to authorise connection attempts.
    pub fn with_mode(mode: BlockListMode) -> Self {
        let (tx, rx) = broadcast::channel(128);

        let inner = Inner {
            mode,
            allow: HashSet::new(),
            block: HashSet::new(),
            tx,
            rx: Some(rx),
        };

        Self {
            inner: Arc::new(RwLock::new(inner)),
        }
    }

    /// Subscribes to events stream.
    pub async fn events(&self) -> broadcast::Receiver<ConnectionBlockListEvent> {
        let mut connection_authoriser = self.inner.write().await;

        let next_rx = connection_authoriser.tx.subscribe();
        connection_authoriser
            .rx
            .replace(next_rx)
            .expect("there's always a receiver")
    }

    /// Sends an event into the events stream.
    ///
    /// All subscribers will be notified of the event.
    async fn send_event(&self, event: ConnectionBlockListEvent) {
        let connection_authoriser = self.inner.write().await;

        // Surpress errors when events are emitted but no event stream subscription exists.
        let _ = connection_authoriser.tx.send(event);
    }

    /// Sets the mode to permissive.
    ///
    /// Any connection will be allowed, as long as it has not been explicitly added to the
    /// blocklist.
    pub async fn permissive(&self) {
        let mut connection_authoriser = self.inner.write().await;
        connection_authoriser.mode = BlockListMode::Permissive;
    }

    /// Sets the mode to restrictive.
    ///
    /// Any connection will be blocked, unless it has been explictly added to the allowlist.
    pub async fn restrictive(&self) {
        let mut connection_authoriser = self.inner.write().await;
        connection_authoriser.mode = BlockListMode::Restrictive;
    }

    /// Allows connections to/from the given node.
    pub async fn allow(&self, node: NodeId) {
        let mut connection_authoriser = self.inner.write().await;
        connection_authoriser.allow.insert(node);
        connection_authoriser.block.remove(&node);
    }

    /// Blocks connections to/from the given node.
    pub async fn block(&self, node: NodeId) {
        let mut connection_authoriser = self.inner.write().await;
        connection_authoriser.block.insert(node);
        connection_authoriser.allow.remove(&node);
    }

    /// Queries the authoriser state for the given node.
    pub async fn can_connect(&self, node: NodeId) -> bool {
        let connection_authoriser = self.inner.read().await;

        match connection_authoriser.mode {
            BlockListMode::Permissive => !connection_authoriser.block.contains(&node),
            BlockListMode::Restrictive => connection_authoriser.allow.contains(&node),
        }
    }
}

impl EndpointHooks for ConnectionBlockList {
    // Runs before an outgoing connection begins.
    async fn before_connect(
        &self,
        remote_addr: &EndpointAddr,
        _alpn: &[u8],
    ) -> BeforeConnectOutcome {
        let node = to_verifying_key(remote_addr.id);

        // Accept or reject the connection attempt based on the authoriser state for the remote
        // node.
        if self.can_connect(node).await {
            self.send_event(ConnectionBlockListEvent::Allowed {
                node,
                role: ConnectionRole::Initiator,
            })
            .await;

            BeforeConnectOutcome::Accept
        } else {
            let event = ConnectionBlockListEvent::Blocked {
                node,
                role: ConnectionRole::Initiator,
            };
            warn!("{}", event);
            self.send_event(event).await;

            BeforeConnectOutcome::Reject
        }
    }

    // Runs after the QUIC/TLS handshake completes for both incoming and outgoing connections.
    //
    // The remote endpoint ID, ALPN, and other metadata are available, but no application data has
    // been sent or received yet.
    async fn after_handshake<'a>(
        &'a self,
        conn: &'a iroh::endpoint::Connection,
    ) -> iroh::endpoint::AfterHandshakeOutcome {
        let node = to_verifying_key(conn.remote_id());
        let role = match conn.side() {
            Side::Server => ConnectionRole::Acceptor,
            Side::Client => ConnectionRole::Initiator,
        };

        if self.can_connect(node).await {
            self.send_event(ConnectionBlockListEvent::Allowed { node, role })
                .await;

            AfterHandshakeOutcome::Accept
        } else {
            let event = ConnectionBlockListEvent::Blocked { node, role };
            warn!("{}", event);
            self.send_event(event).await;

            AfterHandshakeOutcome::Reject {
                error_code: 403u32.into(),
                reason: b"not authorised".into(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use p2panda_core::SigningKey;

    use super::{BlockListMode, ConnectionBlockList};

    #[tokio::test]
    async fn permissive() {
        let list = ConnectionBlockList::default();

        let node_a = SigningKey::generate().verifying_key();
        let node_b = SigningKey::generate().verifying_key();

        assert!(list.can_connect(node_a).await);

        list.block(node_a).await;
        assert!(!list.can_connect(node_a).await);
        assert!(list.can_connect(node_b).await);
    }

    #[tokio::test]
    async fn restrictive() {
        let list = ConnectionBlockList::with_mode(BlockListMode::Restrictive);

        let node_a = SigningKey::generate().verifying_key();
        let node_b = SigningKey::generate().verifying_key();

        assert!(!list.can_connect(node_a).await);

        list.allow(node_a).await;
        assert!(list.can_connect(node_a).await);
        assert!(!list.can_connect(node_b).await);
    }
}
