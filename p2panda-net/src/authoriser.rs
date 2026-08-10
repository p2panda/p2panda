// SPDX-License-Identifier: MIT OR Apache-2.0

//! Authoriser for maintaining and enforcing allowlists and blocklists.
use std::collections::HashSet;
use std::fmt::Display;
use std::sync::Arc;

use p2panda_core::{Topic, VerifyingKey};
use thiserror::Error;
use tokio::sync::RwLock;
use tokio::sync::broadcast::{self, Receiver, Sender};
use tracing::warn;

use crate::NetworkId;
use crate::iroh_endpoint::{BeforeConnectOutcome, EndpointAddr, EndpointHooks};
use crate::utils::{ShortFormat, to_verifying_key};

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
    ConnectionBlocked { node: VerifyingKey },
    ConnectionAllowed { node: VerifyingKey },
    TopicBlocked { topic: Topic, node: VerifyingKey },
    TopicAllowed { topic: Topic, node: VerifyingKey },
}

impl Display for AuthoriserEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthoriserEvent::ConnectionBlocked { node } => {
                write!(f, "blocked connection attempt to {}", node.fmt_short(),)
            }
            AuthoriserEvent::TopicBlocked { topic, node } => {
                write!(
                    f,
                    "blocked connection attempt to {} on topic {}",
                    node.fmt_short(),
                    topic.fmt_short()
                )
            }
            AuthoriserEvent::ConnectionAllowed { node } => {
                write!(f, "allowed connection attempt to {}", node.fmt_short(),)
            }
            AuthoriserEvent::TopicAllowed { topic, node } => {
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

/// Authoriser of connections and sync sessions.
///
/// The authoriser is used to maintain and enforce allowlists and blocklists.
#[derive(Clone, Debug)]
pub struct Authoriser {
    inner: Arc<RwLock<AuthoriserInner>>,
}

#[derive(Clone, Debug)]
pub struct AuthoriserInner {
    mode: AuthoriserMode,
    global_allowlist: HashSet<VerifyingKey>,
    global_blocklist: HashSet<VerifyingKey>,
    topic_allowlist: HashSet<(Topic, VerifyingKey)>,
    topic_blocklist: HashSet<(Topic, VerifyingKey)>,
    tx: Sender<AuthoriserEvent>,
}

impl Default for Authoriser {
    fn default() -> Self {
        Self::new()
    }
}

impl Authoriser {
    /// Returns a connection authoriser and a receiver for authoriser events.
    ///
    /// Defaults to `permissive` mode, meaning that connection attempts from all nodes which are
    /// not explicitly blocked will be accepted.
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel(128);

        let inner = AuthoriserInner {
            mode: AuthoriserMode::Permissive,
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
    pub async fn events(&self) -> Receiver<AuthoriserEvent> {
        let authoriser = self.inner.write().await;
        authoriser.tx.subscribe()
    }

    /// Sends an authoriser event into the events stream.
    ///
    /// All subscribers will be notified of the event.
    pub async fn send_event(&self, event: AuthoriserEvent) {
        let authoriser = self.inner.write().await;

        // Only send the event if there are active receivers.
        //
        // This is primarily to prevent flooding the logs with warnings when events are emitted but
        // no event stream subscription exists.
        if authoriser.tx.receiver_count() > 0 {
            if let Err(err) = authoriser.tx.send(event) {
                warn!("failed to send authoriser event: {}", err)
            } else {
                println!("successfully sent authoriser event")
            }
        }
    }

    /// Sets the authoriser mode to permissive.
    ///
    /// Any connection or sync session with a node or node-topic combination will be allowed, as
    /// long as it has not been explicitly added to the blocklist.
    pub async fn permissive(&self) {
        let mut authoriser = self.inner.write().await;
        authoriser.mode = AuthoriserMode::Permissive;
    }

    /// Sets the authoriser mode to restrictive.
    ///
    /// Any connection or sync session with a node or node-topic combination will be blocked,
    /// unless it has been explictly added to the allowlist.
    pub async fn restrictive(&self) {
        let mut authoriser = self.inner.write().await;
        authoriser.mode = AuthoriserMode::Restrictive;
    }

    /// Allows connections to the given node.
    pub async fn allow(&self, node: VerifyingKey) {
        let mut authoriser = self.inner.write().await;
        authoriser.global_allowlist.insert(node);
        authoriser.global_blocklist.remove(&node);
    }

    /// Blocks connections to the given node.
    pub async fn block(&self, node: VerifyingKey) {
        let mut authoriser = self.inner.write().await;
        authoriser.global_blocklist.insert(node);
        authoriser.global_allowlist.remove(&node);
    }

    /// Allows connections to the given node for a single topic.
    pub async fn topic_allow(&self, node: VerifyingKey, topic: Topic) {
        let mut authoriser = self.inner.write().await;
        authoriser.topic_allowlist.insert((topic, node));
        authoriser.topic_blocklist.remove(&(topic, node));
    }

    /// Blocks connections to the given node for a single topic.
    pub async fn topic_block(&self, node: VerifyingKey, topic: Topic) {
        let mut authoriser = self.inner.write().await;
        authoriser.topic_blocklist.insert((topic, node));
        authoriser.topic_allowlist.remove(&(topic, node));
    }

    /// Queries the authoriser state for the given node-topic combination.
    pub async fn can_connect_on_topic(&self, node: VerifyingKey, topic: Topic) -> bool {
        let authoriser = self.inner.read().await;

        match authoriser.mode {
            AuthoriserMode::Permissive => {
                let global_block = authoriser.global_blocklist.contains(&node);
                let topic_block = authoriser.topic_blocklist.contains(&(topic, node));
                !global_block && !topic_block
            }
            AuthoriserMode::Restrictive => {
                let global_allow = authoriser.global_allowlist.contains(&node);
                let topic_allow = authoriser.topic_allowlist.contains(&(topic, node));

                global_allow && topic_allow
            }
        }
    }

    /// Queries the authoriser state for the given node.
    pub async fn can_connect(&self, node: VerifyingKey) -> bool {
        let authoriser = self.inner.read().await;

        match authoriser.mode {
            AuthoriserMode::Permissive => !authoriser.global_blocklist.contains(&node),
            AuthoriserMode::Restrictive => authoriser.global_allowlist.contains(&node),
        }
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

        // Accept or reject the connection attempt based on the authoriser state for the remote
        // node.
        if self.can_connect(node).await {
            self.send_event(AuthoriserEvent::ConnectionAllowed { node })
                .await;

            BeforeConnectOutcome::Accept
        } else {
            let event = AuthoriserEvent::ConnectionBlocked { node };
            warn!("{}", event);
            self.send_event(event).await;

            BeforeConnectOutcome::Reject
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
pub enum AuthoriserError {
    #[error("not authorised")]
    NotAuthorised,
}

#[cfg(test)]
mod tests {
    use p2panda_core::{SigningKey, Topic};

    use crate::authoriser::Authoriser;

    #[tokio::test]
    async fn authorise_connection_attempts() {
        let authoriser = Authoriser::default();

        let topic_a = Topic::random();
        let topic_b = Topic::random();

        let node_a = SigningKey::generate().verifying_key();
        let node_b = SigningKey::generate().verifying_key();

        assert!(authoriser.can_connect(node_a).await);
        assert!(authoriser.can_connect_on_topic(node_a, topic_a).await);

        authoriser.topic_block(node_a, topic_a).await;
        assert!(authoriser.can_connect(node_a).await);
        assert!(!authoriser.can_connect_on_topic(node_a, topic_a).await);
        assert!(authoriser.can_connect_on_topic(node_a, topic_b).await);
        assert!(authoriser.can_connect_on_topic(node_b, topic_a).await);

        authoriser.block(node_a).await;
        assert!(!authoriser.can_connect(node_a).await);
        assert!(!authoriser.can_connect_on_topic(node_a, topic_a).await);
        assert!(!authoriser.can_connect_on_topic(node_a, topic_b).await);
        assert!(authoriser.can_connect(node_b).await);
        assert!(authoriser.can_connect_on_topic(node_b, topic_a).await);
    }
}
