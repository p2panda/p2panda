// SPDX-License-Identifier: MIT OR Apache-2.0

use tokio::sync::broadcast;

use crate::{NodeId, NodeInfo};

pub type EventsReceiver = broadcast::Receiver<NetworkEvent>;

pub type EventsSender = broadcast::Sender<NetworkEvent>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransportStatus {
    /// Our node can be reached either directly or via a relay.
    ///
    /// We're connected to a relay which guarantees that other nodes can establish a connection
    /// with us, independent of if we're directly reachable or not.
    Online(iroh::EndpointAddr),

    /// Our node _might_ be reachable via a direct address.
    ///
    /// We are _not_ connected to a relay but have direct addresses available. This _might_ be
    /// enough to be reachable for other nodes.
    ///
    /// If our node is running with a directly reachable IP address (no firewalls, no NATs, etc.),
    /// we can be considered "online". If not, we will need a relay. Since we can't distinct
    /// between these two scenarios it is up to the application to decide if this is considered
    /// being "online" or "offline".
    ///
    /// Nodes running on servers can usually consider this event as being "online". Nodes running
    /// on "edge devices" in private networks etc. are probably "offline" in this moment.
    MaybeOnline(iroh::EndpointAddr),

    /// No relay nor direct addresses are available and we can not be reached.
    Offline,
}

impl From<NodeInfo> for TransportStatus {
    fn from(node_info: NodeInfo) -> Self {
        // If there's no iroh-related transport info at all we are "offline".
        let Ok(endpoint_addr) = iroh::EndpointAddr::try_from(node_info) else {
            return TransportStatus::Offline;
        };

        // There's iroh-related info, but it's empty ..
        if endpoint_addr.is_empty() {
            return TransportStatus::Offline;
        }

        if endpoint_addr.relay_urls().next().is_none() {
            return TransportStatus::MaybeOnline(endpoint_addr);
        }

        TransportStatus::Online(endpoint_addr)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelayStatus {
    /// Successfully connected to our home relay.
    Connected(iroh::RelayUrl),

    /// We've changed our home relay.
    Changed(iroh::RelayUrl),

    /// Disconnected from home relay.
    Disconnected,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Protocol {
    Discovery,
    Gossip,
    Sync,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetworkEvent {
    Transport(TransportStatus),
    Relay(RelayStatus),
    Connection {
        protocol: Protocol,
        node_id: NodeId,
        status: ConnectionStatus,
    },
}
