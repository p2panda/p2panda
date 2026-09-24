// SPDX-License-Identifier: MIT OR Apache-2.0

//! Manage lists to allow and block iroh connections.
use std::collections::HashSet;
use std::fmt::Display;
use std::sync::Arc;

use iroh::endpoint::Side;
use p2panda_core::VerifyingKey;
use p2panda_core::traits::ShortFormat;
use tokio::sync::RwLock;
use tokio::sync::broadcast;
use tracing::warn;

use crate::iroh_endpoint::{
    AfterHandshakeOutcome, BeforeConnectOutcome, EndpointAddr, EndpointHooks,
};
use crate::utils::to_verifying_key;

/// Connection authoriser mode for determining how connections are accepted and rejected.
#[derive(Clone, Debug)]
enum ConnectionAuthoriserMode {
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
pub enum ConnectionAuthoriserEvent {
    Blocked {
        node: VerifyingKey,
        role: ConnectionRole,
    },
    Allowed {
        node: VerifyingKey,
        role: ConnectionRole,
    },
}

impl Display for ConnectionAuthoriserEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionAuthoriserEvent::Blocked { node, role } => {
                write!(
                    f,
                    "blocked {} connection attempt to {}",
                    role,
                    node.fmt_short(),
                )
            }
            ConnectionAuthoriserEvent::Allowed { node, role } => {
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

/// Connection authoriser.
///
/// The authoriser is used to maintain and enforce allowlists and blocklists; these can be defined
/// per node (ie. allow or block all connections with a specific node).
#[derive(Clone, Debug)]
pub struct ConnectionAuthoriser {
    inner: Arc<RwLock<ConnectionAuthoriserInner>>,
}

#[derive(Debug)]
struct ConnectionAuthoriserInner {
    mode: ConnectionAuthoriserMode,
    allow: HashSet<VerifyingKey>,
    block: HashSet<VerifyingKey>,
    tx: broadcast::Sender<ConnectionAuthoriserEvent>,
    rx: Option<broadcast::Receiver<ConnectionAuthoriserEvent>>,
}

impl Default for ConnectionAuthoriser {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectionAuthoriser {
    /// Returns a connection authoriser and a receiver for authoriser events.
    ///
    /// Defaults to `permissive` mode, meaning that connection attempts from all nodes which are not
    /// explicitly blocked will be accepted.
    pub fn new() -> Self {
        let (tx, rx) = broadcast::channel(128);

        let inner = ConnectionAuthoriserInner {
            mode: ConnectionAuthoriserMode::Permissive,
            allow: HashSet::new(),
            block: HashSet::new(),
            tx,
            rx: Some(rx),
        };

        Self {
            inner: Arc::new(RwLock::new(inner)),
        }
    }

    /// Subscribes to an authoriser events stream.
    pub async fn events(&self) -> broadcast::Receiver<ConnectionAuthoriserEvent> {
        let mut connection_authoriser = self.inner.write().await;

        let next_rx = connection_authoriser.tx.subscribe();
        connection_authoriser
            .rx
            .replace(next_rx)
            .expect("there's always a receiver")
    }

    /// Sends an authoriser event into the events stream.
    ///
    /// All subscribers will be notified of the event.
    async fn send_event(&self, event: ConnectionAuthoriserEvent) {
        let connection_authoriser = self.inner.write().await;

        // Surpress errors when events are emitted but no event stream subscription exists.
        let _ = connection_authoriser.tx.send(event);
    }

    /// Sets the authoriser mode to permissive.
    ///
    /// Any connection or sync session with a node or node-topic combination will be allowed, as
    /// long as it has not been explicitly added to the blocklist.
    pub async fn permissive(&self) {
        let mut connection_authoriser = self.inner.write().await;
        connection_authoriser.mode = ConnectionAuthoriserMode::Permissive;
    }

    /// Sets the authoriser mode to restrictive.
    ///
    /// Any connection or sync session with a node or node-topic combination will be blocked,
    /// unless it has been explictly added to the allowlist.
    pub async fn restrictive(&self) {
        let mut connection_authoriser = self.inner.write().await;
        connection_authoriser.mode = ConnectionAuthoriserMode::Restrictive;
    }

    /// Allows connections to the given node.
    pub async fn allow(&self, node: VerifyingKey) {
        let mut connection_authoriser = self.inner.write().await;
        connection_authoriser.allow.insert(node);
        connection_authoriser.block.remove(&node);
    }

    /// Blocks connections to the given node.
    pub async fn block(&self, node: VerifyingKey) {
        let mut connection_authoriser = self.inner.write().await;
        connection_authoriser.block.insert(node);
        connection_authoriser.allow.remove(&node);
    }

    /// Queries the authoriser state for the given node.
    pub async fn can_connect(&self, node: VerifyingKey) -> bool {
        let connection_authoriser = self.inner.read().await;

        match connection_authoriser.mode {
            ConnectionAuthoriserMode::Permissive => !connection_authoriser.block.contains(&node),
            ConnectionAuthoriserMode::Restrictive => connection_authoriser.allow.contains(&node),
        }
    }
}

impl EndpointHooks for ConnectionAuthoriser {
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
            self.send_event(ConnectionAuthoriserEvent::Allowed {
                node,
                role: ConnectionRole::Initiator,
            })
            .await;

            BeforeConnectOutcome::Accept
        } else {
            let event = ConnectionAuthoriserEvent::Blocked {
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
            self.send_event(ConnectionAuthoriserEvent::Allowed { node, role })
                .await;

            AfterHandshakeOutcome::Accept
        } else {
            let event = ConnectionAuthoriserEvent::Blocked { node, role };
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
    use p2panda_core::{SigningKey, Topic};

    use crate::connection_authoriser::ConnectionAuthoriser;

    #[tokio::test]
    async fn authorise_connection_attempts() {
        let connection_authoriser = ConnectionAuthoriser::default();

        let node_a = SigningKey::generate().verifying_key();
        let node_b = SigningKey::generate().verifying_key();

        assert!(connection_authoriser.can_connect(node_a).await);

        connection_authoriser.block(node_a).await;
        assert!(!connection_authoriser.can_connect(node_a).await);
        assert!(connection_authoriser.can_connect(node_b).await);
    }
}
