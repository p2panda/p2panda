// SPDX-License-Identifier: MIT OR Apache-2.0

//! Connection authoriser for maintaining and enforcing allowlists and blocklists.
use std::collections::HashSet;
use std::fmt::Display;
use std::sync::Arc;

use iroh::endpoint::Side;
use p2panda_core::{Topic, VerifyingKey};
use thiserror::Error;
use tokio::sync::RwLock;
use tokio::sync::broadcast::{self, Receiver, Sender};
use tracing::warn;

use crate::NetworkId;
use crate::iroh_endpoint::{
    AfterHandshakeOutcome, BeforeConnectOutcome, EndpointAddr, EndpointHooks,
};
use crate::utils::{ShortFormat, to_verifying_key};

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
    TopicBlocked {
        topic: Topic,
        node: VerifyingKey,
    },
    TopicAllowed {
        topic: Topic,
        node: VerifyingKey,
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
            ConnectionAuthoriserEvent::TopicBlocked { topic, node } => {
                write!(
                    f,
                    "blocked connection attempt to {} on topic {}",
                    node.fmt_short(),
                    topic.fmt_short()
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
            ConnectionAuthoriserEvent::TopicAllowed { topic, node } => {
                write!(
                    f,
                    "allowed connection attempt to {} on topic {}",
                    node.fmt_short(),
                    topic.fmt_short()
                )
            }
        }
    }
}

/// Connection authoriser.
///
/// The authoriser is used to maintain and enforce allowlists and blocklists; these can be defined
/// per node (ie. allow or block all connections with a specific node) or per node-topic
/// combinations (ie. allow or block all sync sessions with a specific node for a specific topic).
#[derive(Clone, Debug)]
pub struct ConnectionAuthoriser {
    inner: Arc<RwLock<ConnectionAuthoriserInner>>,
}

#[derive(Clone, Debug)]
pub struct ConnectionAuthoriserInner {
    mode: ConnectionAuthoriserMode,
    global_allowlist: HashSet<VerifyingKey>,
    global_blocklist: HashSet<VerifyingKey>,
    topic_allowlist: HashSet<(Topic, VerifyingKey)>,
    topic_blocklist: HashSet<(Topic, VerifyingKey)>,
    tx: Sender<ConnectionAuthoriserEvent>,
}

impl Default for ConnectionAuthoriser {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectionAuthoriser {
    /// Returns a connection authoriser and a receiver for authoriser events.
    ///
    /// Defaults to `permissive` mode, meaning that connection attempts from all nodes which are
    /// not explicitly blocked will be accepted.
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel(128);

        let inner = ConnectionAuthoriserInner {
            mode: ConnectionAuthoriserMode::Permissive,
            tx,
            global_allowlist: Default::default(),
            global_blocklist: Default::default(),
            topic_allowlist: Default::default(),
            topic_blocklist: Default::default(),
        };

        Self {
            inner: Arc::new(RwLock::new(inner)),
        }
    }

    /// Subscribes to an authoriser events stream.
    pub async fn events(&self) -> Receiver<ConnectionAuthoriserEvent> {
        let connection_authoriser = self.inner.write().await;
        connection_authoriser.tx.subscribe()
    }

    /// Sends an authoriser event into the events stream.
    ///
    /// All subscribers will be notified of the event.
    pub async fn send_event(&self, event: ConnectionAuthoriserEvent) {
        let connection_authoriser = self.inner.write().await;

        // Only send the event if there are active receivers.
        //
        // This is primarily to prevent flooding the logs with warnings when events are emitted but
        // no event stream subscription exists.
        if connection_authoriser.tx.receiver_count() > 0
            && let Err(err) = connection_authoriser.tx.send(event)
        {
            warn!("failed to send authoriser event: {}", err)
        }
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
        connection_authoriser.global_allowlist.insert(node);
        connection_authoriser.global_blocklist.remove(&node);
    }

    /// Blocks connections to the given node.
    pub async fn block(&self, node: VerifyingKey) {
        let mut connection_authoriser = self.inner.write().await;
        connection_authoriser.global_blocklist.insert(node);
        connection_authoriser.global_allowlist.remove(&node);
    }

    /// Allows connections to the given node for a single topic.
    pub async fn topic_allow(&self, node: VerifyingKey, topic: Topic) {
        let mut connection_authoriser = self.inner.write().await;
        connection_authoriser.topic_allowlist.insert((topic, node));
        connection_authoriser.topic_blocklist.remove(&(topic, node));
    }

    /// Blocks connections to the given node for a single topic.
    pub async fn topic_block(&self, node: VerifyingKey, topic: Topic) {
        let mut connection_authoriser = self.inner.write().await;
        connection_authoriser.topic_blocklist.insert((topic, node));
        connection_authoriser.topic_allowlist.remove(&(topic, node));
    }

    /// Queries the authoriser state for the given node-topic combination.
    pub async fn can_connect_on_topic(&self, node: VerifyingKey, topic: Topic) -> bool {
        let connection_authoriser = self.inner.read().await;

        match connection_authoriser.mode {
            ConnectionAuthoriserMode::Permissive => {
                let global_block = connection_authoriser.global_blocklist.contains(&node);
                let topic_block = connection_authoriser
                    .topic_blocklist
                    .contains(&(topic, node));
                !global_block && !topic_block
            }
            ConnectionAuthoriserMode::Restrictive => {
                let global_allow = connection_authoriser.global_allowlist.contains(&node);
                let topic_allow = connection_authoriser
                    .topic_allowlist
                    .contains(&(topic, node));

                global_allow && topic_allow
            }
        }
    }

    /// Queries the authoriser state for the given node.
    pub async fn can_connect(&self, node: VerifyingKey) -> bool {
        let connection_authoriser = self.inner.read().await;

        match connection_authoriser.mode {
            ConnectionAuthoriserMode::Permissive => {
                !connection_authoriser.global_blocklist.contains(&node)
            }
            ConnectionAuthoriserMode::Restrictive => {
                connection_authoriser.global_allowlist.contains(&node)
            }
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
    // The remote endpoint ID, ALPN, and other metadata are available, but no application data has been sent or received yet.
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

/// Hash the concatenation of the given topic and network id.
pub fn hash_topic_with_network_id(topic: Topic, network_id: NetworkId) -> Vec<u8> {
    p2panda_core::Hash::digest([topic.to_bytes().as_ref(), &network_id].concat())
        .as_bytes()
        .to_vec()
}

#[derive(Debug, Error)]
pub enum ConnectionAuthoriserError {
    #[error("not authorised")]
    NotAuthorised,
}

#[cfg(test)]
mod tests {
    use p2panda_core::{SigningKey, Topic};

    use crate::connection_authoriser::ConnectionAuthoriser;

    #[tokio::test]
    async fn authorise_connection_attempts() {
        let connection_authoriser = ConnectionAuthoriser::default();

        let topic_a = Topic::random();
        let topic_b = Topic::random();

        let node_a = SigningKey::generate().verifying_key();
        let node_b = SigningKey::generate().verifying_key();

        assert!(connection_authoriser.can_connect(node_a).await);
        assert!(
            connection_authoriser
                .can_connect_on_topic(node_a, topic_a)
                .await
        );

        connection_authoriser.topic_block(node_a, topic_a).await;
        assert!(connection_authoriser.can_connect(node_a).await);
        assert!(
            !connection_authoriser
                .can_connect_on_topic(node_a, topic_a)
                .await
        );
        assert!(
            connection_authoriser
                .can_connect_on_topic(node_a, topic_b)
                .await
        );
        assert!(
            connection_authoriser
                .can_connect_on_topic(node_b, topic_a)
                .await
        );

        connection_authoriser.block(node_a).await;
        assert!(!connection_authoriser.can_connect(node_a).await);
        assert!(
            !connection_authoriser
                .can_connect_on_topic(node_a, topic_a)
                .await
        );
        assert!(
            !connection_authoriser
                .can_connect_on_topic(node_a, topic_b)
                .await
        );
        assert!(connection_authoriser.can_connect(node_b).await);
        assert!(
            connection_authoriser
                .can_connect_on_topic(node_b, topic_a)
                .await
        );
    }
}
