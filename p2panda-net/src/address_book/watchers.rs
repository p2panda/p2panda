// SPDX-License-Identifier: MIT OR Apache-2.0

use std::cell::RefCell;
use std::collections::HashSet;

use tracing::debug;

use crate::addrs::NodeInfo;
use crate::utils::ShortFormat;
use crate::watchers::{UpdateResult, Watched, WatchedValue};
use crate::{NodeId, TopicId};

/// Watch for changes of a node's info.
#[derive(Default)]
pub struct WatchedNodeInfo(RefCell<Option<NodeInfo>>);

impl WatchedNodeInfo {
    pub fn from_node_info(node_info: Option<NodeInfo>) -> Self {
        Self(RefCell::new(node_info))
    }
}

impl Watched for WatchedNodeInfo {
    type Value = Option<NodeInfo>;

    fn current(&self) -> Self::Value {
        self.0.borrow().clone()
    }

    fn update_if_changed(&self, cmp: &Self::Value) -> UpdateResult<Self::Value> {
        if !self.0.borrow().eq(cmp) {
            if let Some(info) = cmp {
                let transports = info
                    .transports
                    .as_ref()
                    .map(|info| info.to_string())
                    .unwrap_or("none".to_string());
                debug!(
                    node_id = info.node_id.fmt_short(),
                    %transports,
                    "node info changed"
                );
            }

            self.0.replace(cmp.to_owned());

            UpdateResult::Changed(WatchedValue {
                difference: None,
                value: cmp.to_owned(),
            })
        } else {
            UpdateResult::Unchanged
        }
    }
}

/// Watch for changes of nodes being interested in a topic.
pub struct WatchedTopic {
    topic: TopicId,
    node_ids: RefCell<HashSet<NodeId>>,
}

impl WatchedTopic {
    pub fn from_node_ids(topic: TopicId, node_ids: HashSet<NodeId>) -> Self {
        Self {
            topic,
            node_ids: RefCell::new(node_ids),
        }
    }
}

impl Watched for WatchedTopic {
    type Value = HashSet<NodeId>;

    fn current(&self) -> Self::Value {
        self.node_ids.borrow().clone()
    }

    fn update_if_changed(&self, cmp: &Self::Value) -> UpdateResult<Self::Value> {
        let difference: HashSet<NodeId> = self
            .node_ids
            .borrow()
            .symmetric_difference(cmp)
            .cloned()
            .collect();

        if difference.is_empty() {
            UpdateResult::Unchanged
        } else {
            self.node_ids.replace(cmp.to_owned());

            {
                let node_ids: Vec<String> = self
                    .node_ids
                    .borrow()
                    .iter()
                    .map(|id| id.fmt_short())
                    .collect();
                debug!(
                    topic = self.topic.fmt_short(),
                    node_ids = ?node_ids,
                    "interested nodes for topic changed"
                );
            }

            UpdateResult::Changed(WatchedValue {
                difference: Some(difference),
                value: cmp.to_owned(),
            })
        }
    }
}

/// Watch for changes of topics for a node.
pub struct WatchedNodeTopics {
    node_id: NodeId,
    topic_ids: RefCell<HashSet<TopicId>>,
}

impl WatchedNodeTopics {
    pub fn from_topics(node_id: NodeId, topic_ids: HashSet<TopicId>) -> Self {
        Self {
            node_id,
            topic_ids: RefCell::new(topic_ids),
        }
    }
}

impl Watched for WatchedNodeTopics {
    type Value = HashSet<TopicId>;

    fn current(&self) -> Self::Value {
        self.topic_ids.borrow().clone()
    }

    fn update_if_changed(&self, cmp: &Self::Value) -> UpdateResult<Self::Value> {
        let difference: HashSet<TopicId> = self
            .topic_ids
            .borrow()
            .symmetric_difference(cmp)
            .cloned()
            .collect();

        if difference.is_empty() {
            UpdateResult::Unchanged
        } else {
            self.topic_ids.replace(cmp.to_owned());

            {
                let topic_ids: Vec<String> = self
                    .topic_ids
                    .borrow()
                    .iter()
                    .map(|id| id.fmt_short())
                    .collect();
                debug!(
                    node_id = self.node_id.fmt_short(),
                    topic_ids = ?topic_ids,
                    "topics for node changed"
                );
            }

            UpdateResult::Changed(WatchedValue {
                difference: Some(difference),
                value: cmp.to_owned(),
            })
        }
    }
}
