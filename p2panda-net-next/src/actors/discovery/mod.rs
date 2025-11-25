// SPDX-License-Identifier: MIT OR Apache-2.0

mod manager;
mod session;
#[cfg(test)]
mod tests;
mod walker;

use std::time::Duration;

use p2panda_discovery::DiscoveryResult;
use ractor::{ActorCell, ActorRef};

use crate::actors::discovery::session::{DISCOVERY_SESSION, DiscoverySessionId};
use crate::actors::discovery::walker::DISCOVERY_WALKER;
use crate::actors::{ActorNamespace, with_namespace, without_namespace};
use crate::{NodeId, NodeInfo};

pub use manager::{DISCOVERY_MANAGER, DiscoveryManager, ToDiscoveryManager};

pub const DISCOVERY_PROTOCOL_ID: &[u8] = b"p2panda/discovery/v1";

/// Helper to extract information about an actor given it's name (just a string).
#[derive(Debug, PartialEq)]
enum DiscoveryActorName {
    Walker { walker_id: usize },
    Session { session_id: DiscoverySessionId },
    AcceptedSession { session_id: DiscoverySessionId },
}

impl DiscoveryActorName {
    pub fn new_walker(walker_id: usize) -> Self {
        Self::Walker { walker_id }
    }

    pub fn new_session(session_id: DiscoverySessionId) -> Self {
        Self::Session { session_id }
    }

    pub fn new_accept_session(session_id: DiscoverySessionId) -> Self {
        Self::AcceptedSession { session_id }
    }

    fn from_string(name: &str) -> Self {
        let name = without_namespace(name);
        if name.contains(DISCOVERY_WALKER) {
            Self::Walker {
                walker_id: Self::extract_id(name) as usize,
            }
        } else if name.contains(DISCOVERY_SESSION) {
            Self::Session {
                session_id: Self::extract_id(name),
            }
        } else {
            unreachable!("actors have either walker or session name")
        }
    }

    pub fn from_actor_cell(actor_cell: &ActorCell) -> Self {
        Self::from_string(&actor_cell.get_name().expect("actor needs to have a name"))
    }

    pub fn from_actor_ref<T>(actor_ref: &ActorRef<T>) -> Self {
        Self::from_string(&actor_ref.get_name().expect("actor needs to have a name"))
    }

    fn extract_id(actor_name: &str) -> u64 {
        let Some((_, suffix)) = actor_name.rsplit_once('.') else {
            unreachable!("actors have all the same name pattern")
        };
        suffix.parse::<u64>().expect("suffix is a number")
    }

    pub fn walker_id(&self) -> usize {
        match self {
            DiscoveryActorName::Walker { walker_id } => *walker_id,
            _ => unreachable!("should only be called on walker actors"),
        }
    }

    pub fn to_string(&self, actor_namespace: &ActorNamespace) -> String {
        match self {
            DiscoveryActorName::Walker { walker_id } => {
                with_namespace(&format!("{DISCOVERY_WALKER}.{walker_id}"), actor_namespace)
            }
            DiscoveryActorName::Session { session_id }
            | DiscoveryActorName::AcceptedSession { session_id } => with_namespace(
                &format!("{DISCOVERY_SESSION}.{session_id}"),
                actor_namespace,
            ),
        }
    }
}

/// Discovery "system" events other processes can subscribe to.
#[derive(Clone, Debug, PartialEq, Eq)]
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
