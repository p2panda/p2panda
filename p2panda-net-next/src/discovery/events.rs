// SPDX-License-Identifier: MIT OR Apache-2.0

use std::time::Duration;

use p2panda_discovery::DiscoveryResult;

use crate::NodeId;
use crate::addrs::NodeInfo;

/// Discovery "system" events other processes can subscribe to.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
#[allow(clippy::large_enum_variant)]
pub enum DiscoveryEvent {
    SessionStarted {
        role: SessionRole,
        remote_node_id: NodeId,
    },
    SessionEnded {
        role: SessionRole,
        remote_node_id: NodeId,
        result: DiscoveryResult<NodeId, NodeInfo>,
        duration: Duration,
    },
    SessionFailed {
        role: SessionRole,
        remote_node_id: NodeId,
        duration: Duration,
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionRole {
    Initiated,
    Accepted,
}
