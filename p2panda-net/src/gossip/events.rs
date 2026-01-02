// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;

use crate::{NodeId, TopicId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GossipEvent {
    Joined {
        topic: TopicId,
        nodes: HashSet<NodeId>,
    },
    NeighbourUp {
        node: NodeId,
        topic: TopicId,
    },
    NeighbourDown {
        node: NodeId,
        topic: TopicId,
    },
    Left {
        topic: TopicId,
    },
}
