// SPDX-License-Identifier: MIT OR Apache-2.0

//! Manage lists to allow and block sync sessions.
use std::collections::HashSet;
use std::fmt::Display;
use std::sync::Arc;

use p2panda_core::traits::ShortFormat;
use p2panda_core::{Topic, VerifyingKey};
use tokio::sync::{RwLock, broadcast};

use crate::NodeId;
use crate::sync::hooks::{AfterHandshakeOutcome, SyncHooks};

/// Determines if the block/allow list is permissive or restrictive.
#[derive(Clone, Debug)]
pub enum SyncBlockListMode {
    /// Allow all sync sessions except for nodes which have been explicitly blocked.
    Permissive,

    /// Block all sync sessions except for nodes which have been explicitly allowed.
    Restrictive,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncBlockListEvent {
    Blocked {
        remote_node_id: NodeId,
        topic: Topic,
    },
    Allowed {
        remote_node_id: NodeId,
        topic: Topic,
    },
}

impl Display for SyncBlockListEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncBlockListEvent::Blocked {
                remote_node_id,
                topic,
            } => {
                write!(
                    f,
                    "blocked sync attempt with {} on topic {}",
                    remote_node_id.fmt_short(),
                    topic.fmt_short()
                )
            }
            SyncBlockListEvent::Allowed {
                remote_node_id,
                topic,
            } => {
                write!(
                    f,
                    "allowed sync attempt with {} on topic {}",
                    remote_node_id.fmt_short(),
                    topic.fmt_short()
                )
            }
        }
    }
}

/// Accept or reject sync sessions with a managed allow/block list.
#[derive(Clone, Debug)]
pub struct SyncBlockList {
    inner: Arc<RwLock<Inner>>,
}

#[derive(Debug)]
struct Inner {
    mode: SyncBlockListMode,
    global_allowlist: HashSet<VerifyingKey>,
    global_blocklist: HashSet<VerifyingKey>,
    topic_allowlist: HashSet<(Topic, VerifyingKey)>,
    topic_blocklist: HashSet<(Topic, VerifyingKey)>,
    tx: broadcast::Sender<SyncBlockListEvent>,
    rx: Option<broadcast::Receiver<SyncBlockListEvent>>,
}

impl Default for SyncBlockList {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncBlockList {
    /// Returns a sync authoriser.
    ///
    /// Defaults to `permissive` mode, meaning that sync attempts from all nodes which are not
    /// explicitly blocked will be accepted.
    pub fn new() -> Self {
        Self::with_mode(SyncBlockListMode::Permissive)
    }

    /// Returns a sync authoriser.
    pub fn with_mode(mode: SyncBlockListMode) -> Self {
        let (tx, rx) = broadcast::channel(128);

        let inner = Inner {
            mode,
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
    pub async fn events(&self) -> broadcast::Receiver<SyncBlockListEvent> {
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
    async fn send_event(&self, event: SyncBlockListEvent) {
        let inner = self.inner.write().await;

        // Surpress errors when events are emitted but no event stream subscription exists.
        let _ = inner.tx.send(event);
    }

    /// Sets the mode to permissive.
    ///
    /// Any sync session with a node or node-topic combination will be allowed, as long as it has
    /// not been explicitly added to the blocklist.
    pub async fn permissive(&self) {
        let mut inner = self.inner.write().await;
        inner.mode = SyncBlockListMode::Permissive;
    }

    /// Sets the mode to restrictive.
    ///
    /// Any sync session with a node or node-topic combination will be blocked, unless it has been
    /// explictly added to the allowlist.
    pub async fn restrictive(&self) {
        let mut inner = self.inner.write().await;
        inner.mode = SyncBlockListMode::Restrictive;
    }

    /// Allows sync sessions with the given node.
    pub async fn allow(&self, node: VerifyingKey) {
        let mut inner = self.inner.write().await;
        inner.global_allowlist.insert(node);
        inner.global_blocklist.remove(&node);
    }

    /// Blocks sync sessions with the given node.
    pub async fn block(&self, node: VerifyingKey) {
        let mut inner = self.inner.write().await;
        inner.global_blocklist.insert(node);
        inner.global_allowlist.remove(&node);
    }

    /// Allows sync sessions with the given node for a single topic.
    pub async fn allow_topic(&self, node: VerifyingKey, topic: Topic) {
        let mut inner = self.inner.write().await;
        inner.topic_allowlist.insert((topic, node));
        inner.topic_blocklist.remove(&(topic, node));
    }

    /// Blocks sync sessions with the given node for a single topic.
    pub async fn block_topic(&self, node: VerifyingKey, topic: Topic) {
        let mut inner = self.inner.write().await;
        inner.topic_blocklist.insert((topic, node));
        inner.topic_allowlist.remove(&(topic, node));
    }

    /// Queries the authoriser state for the given node-topic combination.
    pub async fn can_sync(&self, node: VerifyingKey, topic: Topic) -> bool {
        let inner = self.inner.read().await;

        match inner.mode {
            SyncBlockListMode::Permissive => {
                let global_block = inner.global_blocklist.contains(&node);

                if global_block {
                    inner.topic_allowlist.contains(&(topic, node))
                } else {
                    let topic_block = inner.topic_blocklist.contains(&(topic, node));
                    !topic_block
                }
            }
            SyncBlockListMode::Restrictive => {
                let global_allow = inner.global_allowlist.contains(&node);

                if global_allow {
                    let topic_block = inner.topic_blocklist.contains(&(topic, node));
                    !topic_block
                } else {
                    inner.topic_allowlist.contains(&(topic, node))
                }
            }
        }
    }
}

impl SyncHooks for SyncBlockList {
    type Handshake = Topic;

    // Runs before an outgoing connection begins.
    async fn after_handshake(
        &self,
        remote_node_id: NodeId,
        topic: &Topic,
    ) -> AfterHandshakeOutcome {
        if self.can_sync(remote_node_id, *topic).await {
            self.send_event(SyncBlockListEvent::Allowed {
                remote_node_id,
                topic: *topic,
            })
            .await;

            AfterHandshakeOutcome::Accept
        } else {
            self.send_event(SyncBlockListEvent::Blocked {
                remote_node_id,
                topic: *topic,
            })
            .await;

            AfterHandshakeOutcome::Reject
        }
    }
}

#[cfg(test)]
mod tests {
    use p2panda_core::{SigningKey, Topic};

    use super::{SyncBlockList, SyncBlockListMode};

    #[tokio::test]
    async fn permissive() {
        let list = SyncBlockList::default();

        let node_a = SigningKey::generate().verifying_key();
        let node_b = SigningKey::generate().verifying_key();

        let bears = Topic::random();
        let beavers = Topic::random();

        // In permissive mode anyone can sync anything without being explicitly blocked.
        assert!(list.can_sync(node_a, bears).await);
        assert!(list.can_sync(node_a, beavers).await);
        assert!(list.can_sync(node_b, bears).await);
        assert!(list.can_sync(node_b, beavers).await);

        // Any topic is blocked for Node A.
        list.block(node_a).await;
        assert!(!list.can_sync(node_a, bears).await);
        assert!(!list.can_sync(node_a, beavers).await);

        // Node B can stil sync anything.
        assert!(list.can_sync(node_b, bears).await);
        assert!(list.can_sync(node_b, beavers).await);

        // Allow Node A to _only_ sync "beavers".
        list.allow_topic(node_a, beavers).await;
        assert!(!list.can_sync(node_a, bears).await);
        assert!(list.can_sync(node_a, beavers).await);

        // Block Node B to not  sync "beavers".
        list.block_topic(node_b, beavers).await;
        assert!(!list.can_sync(node_b, beavers).await);
        assert!(list.can_sync(node_b, bears).await);
    }

    #[tokio::test]
    async fn restrictive() {
        let list = SyncBlockList::with_mode(SyncBlockListMode::Restrictive);

        let node_a = SigningKey::generate().verifying_key();
        let node_b = SigningKey::generate().verifying_key();

        let squirrels = Topic::random();
        let wolves = Topic::random();

        // In permissive mode nobody can sync anything without being explicitly allowed.
        assert!(!list.can_sync(node_a, squirrels).await);
        assert!(!list.can_sync(node_a, wolves).await);
        assert!(!list.can_sync(node_b, squirrels).await);
        assert!(!list.can_sync(node_b, wolves).await);

        // Node A is allowed to sync any topic.
        list.allow(node_a).await;
        assert!(list.can_sync(node_a, squirrels).await);
        assert!(list.can_sync(node_a, wolves).await);

        // Node B is still blocked.
        assert!(!list.can_sync(node_b, squirrels).await);
        assert!(!list.can_sync(node_b, wolves).await);

        // Allow Node B to sync "wolves".
        list.allow_topic(node_b, wolves).await;
        assert!(list.can_sync(node_b, wolves).await);
        assert!(!list.can_sync(node_b, squirrels).await);

        // Block Node A to sync "squirrels".
        list.block_topic(node_b, squirrels).await;
        assert!(list.can_sync(node_b, wolves).await);
        assert!(!list.can_sync(node_b, squirrels).await);
    }
}
