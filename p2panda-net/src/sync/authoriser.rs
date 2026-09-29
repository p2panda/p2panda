// SPDX-License-Identifier: MIT OR Apache-2.0

//! Manage lists to allow and block sync sessions.
use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::sync::Arc;

use p2panda_core::Topic;
use p2panda_core::traits::ShortFormat;
use tokio::sync::{RwLock, broadcast};

use crate::NodeId;
use crate::sync::hooks::{AfterHandshakeOutcome, SyncHooks};

/// Determines if the block/allow list is permissive or restrictive.
#[derive(Clone, Debug)]
pub enum BlockListMode {
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
    mode: BlockListMode,
    global_allowlist: HashSet<NodeId>,
    global_blocklist: HashSet<NodeId>,
    topic_allowlist: TopicList,
    topic_blocklist: TopicList,
    tx: broadcast::Sender<SyncBlockListEvent>,
    rx: Option<broadcast::Receiver<SyncBlockListEvent>>,
}

#[derive(Debug)]
struct TopicList(HashMap<NodeId, HashSet<Topic>>);

impl TopicList {
    pub fn new() -> Self {
        Self(HashMap::default())
    }

    pub fn insert(&mut self, node: NodeId, topic: Topic) {
        self.0
            .entry(node)
            .and_modify(|list| {
                list.insert(topic);
            })
            .or_insert(HashSet::from([topic]));
    }

    pub fn remove_topic(&mut self, node: NodeId, topic: Topic) {
        if let Some(list) = self.0.get_mut(&node) {
            list.remove(&topic);
        }
    }

    pub fn remove_node(&mut self, node: NodeId) {
        self.0.remove(&node);
    }

    pub fn contains(&self, node: NodeId, topic: Topic) -> bool {
        match self.0.get(&node) {
            Some(list) => list.contains(&topic),
            None => false,
        }
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
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
        Self::with_mode(BlockListMode::Permissive)
    }

    /// Returns a sync authoriser.
    pub fn with_mode(mode: BlockListMode) -> Self {
        let (tx, rx) = broadcast::channel(128);

        let inner = Inner {
            mode,
            global_allowlist: HashSet::default(),
            global_blocklist: HashSet::default(),
            topic_allowlist: TopicList::new(),
            topic_blocklist: TopicList::new(),
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
        inner.mode = BlockListMode::Permissive;
    }

    /// Sets the mode to restrictive.
    ///
    /// Any sync session with a node or node-topic combination will be blocked, unless it has been
    /// explictly added to the allowlist.
    pub async fn restrictive(&self) {
        let mut inner = self.inner.write().await;
        inner.mode = BlockListMode::Restrictive;
    }

    /// Allows sync sessions with the given node.
    ///
    /// This removes all previously blocked topics for this node.
    pub async fn allow(&self, node: NodeId) {
        let mut inner = self.inner.write().await;
        inner.global_allowlist.insert(node);
        inner.global_blocklist.remove(&node);

        // Clear list from all explicitly blocked topics for this node.
        inner.topic_blocklist.remove_node(node);
    }

    /// Blocks sync sessions with the given node.
    ///
    /// This removes all previously allowed topics for this node.
    pub async fn block(&self, node: NodeId) {
        let mut inner = self.inner.write().await;
        inner.global_blocklist.insert(node);
        inner.global_allowlist.remove(&node);

        // Clear list from all explicitly allowed topics for this node.
        inner.topic_allowlist.remove_node(node);
    }

    /// Allows sync sessions with the given node for a single topic.
    pub async fn allow_topic(&self, node: NodeId, topic: Topic) {
        let mut inner = self.inner.write().await;
        inner.topic_allowlist.insert(node, topic);
        inner.topic_blocklist.remove_topic(node, topic);
    }

    /// Blocks sync sessions with the given node for a single topic.
    pub async fn block_topic(&self, node: NodeId, topic: Topic) {
        let mut inner = self.inner.write().await;
        inner.topic_blocklist.insert(node, topic);
        inner.topic_allowlist.remove_topic(node, topic);
    }

    /// Clears all lists.
    pub async fn clear(&self) {
        let mut inner = self.inner.write().await;
        inner.global_blocklist.clear();
        inner.global_allowlist.clear();
        inner.topic_blocklist.clear();
        inner.topic_allowlist.clear();
    }

    /// Queries the authoriser state for the given node-topic combination.
    pub async fn can_sync(&self, node: NodeId, topic: Topic) -> bool {
        let inner = self.inner.read().await;

        match inner.mode {
            BlockListMode::Permissive => {
                let global_block = inner.global_blocklist.contains(&node);

                if global_block {
                    inner.topic_allowlist.contains(node, topic)
                } else {
                    let topic_block = inner.topic_blocklist.contains(node, topic);
                    !topic_block
                }
            }
            BlockListMode::Restrictive => {
                let global_allow = inner.global_allowlist.contains(&node);

                if global_allow {
                    let topic_block = inner.topic_blocklist.contains(node, topic);
                    !topic_block
                } else {
                    inner.topic_allowlist.contains(node, topic)
                }
            }
        }
    }
}

impl SyncHooks for SyncBlockList {
    type Handshake = Topic;

    // Runs after the handshake of an incoming or outgoing sync session finished and before sync
    // begins.
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

    use super::{BlockListMode, SyncBlockList};

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

        // Block Node B to not sync "beavers".
        list.block_topic(node_b, beavers).await;
        assert!(!list.can_sync(node_b, beavers).await);
        assert!(list.can_sync(node_b, bears).await);

        // Block A globally again, this should reset all previously allowed topics.
        list.block(node_a).await;
        assert!(!list.can_sync(node_a, bears).await);
        assert!(!list.can_sync(node_a, beavers).await);
    }

    #[tokio::test]
    async fn restrictive() {
        let list = SyncBlockList::with_mode(BlockListMode::Restrictive);

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
        list.block_topic(node_a, squirrels).await;
        assert!(list.can_sync(node_a, wolves).await);
        assert!(!list.can_sync(node_a, squirrels).await);

        // Allow all topics for A globally again, this should reset all previously blocked topics.
        list.allow(node_a).await;
        assert!(list.can_sync(node_a, wolves).await);
        assert!(list.can_sync(node_a, squirrels).await);
    }
}
