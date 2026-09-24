// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;
use std::fmt::Display;
use std::sync::Arc;

use p2panda_core::traits::ShortFormat;
use p2panda_core::{Topic, VerifyingKey};
use thiserror::Error;
use tokio::sync::{RwLock, broadcast};

/// Sync authoriser mode for determining how connections are accepted and rejected.
#[derive(Clone, Debug)]
enum SyncAuthoriserMode {
    /// Allow all connections except for nodes which have been explicitly blocked.
    Permissive,

    /// Block all connections except for nodes which have been explicitly allowed.
    Restrictive,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncAuthoriserEvent {
    TopicBlocked { topic: Topic, node: VerifyingKey },
    TopicAllowed { topic: Topic, node: VerifyingKey },
}

impl Display for SyncAuthoriserEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncAuthoriserEvent::TopicBlocked { topic, node } => {
                write!(
                    f,
                    "blocked sync attempt to {} on topic {}",
                    node.fmt_short(),
                    topic.fmt_short()
                )
            }
            SyncAuthoriserEvent::TopicAllowed { topic, node } => {
                write!(
                    f,
                    "allowed sync attempt to {} on topic {}",
                    node.fmt_short(),
                    topic.fmt_short()
                )
            }
        }
    }
}

/// Sync authoriser.
///
/// The authoriser is used to maintain and enforce allowlists and blocklists; these can be defined
/// per node (ie. allow or block all connections with a specific node) or per node-topic
/// combinations (ie. allow or block all sync sessions with a specific node for a specific topic).
#[derive(Clone, Debug)]
pub struct SyncAuthoriser {
    inner: Arc<RwLock<SyncAuthoriserInner>>,
}

#[derive(Debug)]
struct SyncAuthoriserInner {
    mode: SyncAuthoriserMode,
    global_allowlist: HashSet<VerifyingKey>,
    global_blocklist: HashSet<VerifyingKey>,
    topic_allowlist: HashSet<(Topic, VerifyingKey)>,
    topic_blocklist: HashSet<(Topic, VerifyingKey)>,
    tx: broadcast::Sender<SyncAuthoriserEvent>,
    rx: Option<broadcast::Receiver<SyncAuthoriserEvent>>,
}

impl Default for SyncAuthoriser {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncAuthoriser {
    /// Returns a sync authoriser and a receiver for authoriser events.
    ///
    /// Defaults to `permissive` mode, meaning that sync attempts from all nodes which are
    /// not explicitly blocked will be accepted.
    pub fn new() -> Self {
        let (tx, rx) = broadcast::channel(128);

        let inner = SyncAuthoriserInner {
            mode: SyncAuthoriserMode::Permissive,
            global_allowlist: HashSet::default(),
            global_blocklist: HashSet::default(),
            topic_allowlist: HashSet::default(),
            topic_blocklist: HashSet::default(),
            tx,
            rx: Some(rx),
        };

        Self {
            inner: Arc::new(RwLock::new(inner)),
        }
    }

    /// Subscribes to an authoriser events stream.
    pub async fn events(&self) -> broadcast::Receiver<SyncAuthoriserEvent> {
        let mut inner = self.inner.write().await;

        let next_rx = inner.tx.subscribe();
        inner
            .rx
            .replace(next_rx)
            .expect("there's always a receiver")
    }

    /// Sends an authoriser event into the events stream.
    ///
    /// All subscribers will be notified of the event.
    pub(crate) async fn send_event(&self, event: SyncAuthoriserEvent) {
        let inner = self.inner.write().await;

        // Surpress errors when events are emitted but no event stream subscription exists.
        let _ = inner.tx.send(event);
    }

    /// Sets the authoriser mode to permissive.
    ///
    /// Any sync or sync session with a node or node-topic combination will be allowed, as
    /// long as it has not been explicitly added to the blocklist.
    pub async fn permissive(&self) {
        let mut inner = self.inner.write().await;
        inner.mode = SyncAuthoriserMode::Permissive;
    }

    /// Sets the authoriser mode to restrictive.
    ///
    /// Any sync or sync session with a node or node-topic combination will be blocked,
    /// unless it has been explictly added to the allowlist.
    pub async fn restrictive(&self) {
        let mut inner = self.inner.write().await;
        inner.mode = SyncAuthoriserMode::Restrictive;
    }

    /// Allows connections to the given node.
    pub async fn allow(&self, node: VerifyingKey) {
        let mut inner = self.inner.write().await;
        inner.global_allowlist.insert(node);
        inner.global_blocklist.remove(&node);
    }

    /// Blocks connections to the given node.
    pub async fn block(&self, node: VerifyingKey) {
        let mut inner = self.inner.write().await;
        inner.global_blocklist.insert(node);
        inner.global_allowlist.remove(&node);
    }

    /// Allows connections to the given node for a single topic.
    pub async fn topic_allow(&self, node: VerifyingKey, topic: Topic) {
        let mut inner = self.inner.write().await;
        inner.topic_allowlist.insert((topic, node));
        inner.topic_blocklist.remove(&(topic, node));
    }

    /// Blocks connections to the given node for a single topic.
    pub async fn topic_block(&self, node: VerifyingKey, topic: Topic) {
        let mut inner = self.inner.write().await;
        inner.topic_blocklist.insert((topic, node));
        inner.topic_allowlist.remove(&(topic, node));
    }

    /// Queries the authoriser state for the given node-topic combination.
    pub async fn can_connect_on_topic(&self, node: VerifyingKey, topic: Topic) -> bool {
        let inner = self.inner.read().await;

        match inner.mode {
            SyncAuthoriserMode::Permissive => {
                let global_block = inner.global_blocklist.contains(&node);
                let topic_block = inner.topic_blocklist.contains(&(topic, node));
                !global_block && !topic_block
            }
            SyncAuthoriserMode::Restrictive => {
                let global_allow = inner.global_allowlist.contains(&node);
                let topic_allow = inner.topic_allowlist.contains(&(topic, node));

                global_allow && topic_allow
            }
        }
    }

    /// Queries the authoriser state for the given node.
    pub async fn can_connect(&self, node: VerifyingKey) -> bool {
        let inner = self.inner.read().await;

        match inner.mode {
            SyncAuthoriserMode::Permissive => !inner.global_blocklist.contains(&node),
            SyncAuthoriserMode::Restrictive => inner.global_allowlist.contains(&node),
        }
    }
}

#[derive(Debug, Error)]
pub enum SyncAuthoriserError {
    #[error("not authorised")]
    NotAuthorised,
}
