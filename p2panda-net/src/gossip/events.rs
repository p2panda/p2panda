// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;

use p2panda_core::Topic;

use crate::NodeId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GossipEvent {
    Joined {
        topic: Topic,
        nodes: HashSet<NodeId>,
    },
    NeighbourUp {
        node: NodeId,
        topic: Topic,
    },
    NeighbourDown {
        node: NodeId,
        topic: Topic,
    },
    Left {
        topic: Topic,
    },
}
