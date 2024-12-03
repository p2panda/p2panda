// SPDX-License-Identifier: AGPL-3.0-or-later

//! Alternative configuration API which can be passed into `Network::from_config` constructor
//! instead of using `NetworkBuilder`.
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{NetworkId, NodeAddress, RelayUrl};

/// Default port of a node socket.
pub const DEFAULT_BIND_PORT: u16 = 2022;

/// Default network id.
pub const DEFAULT_NETWORK_ID: NetworkId = [
    247, 69, 248, 242, 132, 120, 159, 230, 98, 100, 214, 200, 78, 40, 79, 94, 174, 8, 12, 27, 84,
    195, 246, 159, 132, 240, 79, 208, 1, 43, 132, 118,
];

/// Configuration parameters for the local network node.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    /// Bind port for the IPv4 socket. The IPv6 socket will be bound to `bind_port` + 1.
    pub bind_port: u16,

    /// Node addresses of remote peers which are directly reachable (no STUN or relay required).
    /// These will be added to the address book.
    pub direct_node_addresses: Vec<NodeAddress>,

    /// Identifier of the network to be joined.
    pub network_id: NetworkId,

    /// Path to the local private key. If not provided, a random keypair will be generated and kept
    /// in memory.
    pub private_key: Option<PathBuf>,

    /// URL of a relay server to help in establishing a peer-to-peer connection if one or both peers
    /// are behind a NAT or firewall.
    pub relay: Option<RelayUrl>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind_port: DEFAULT_BIND_PORT,
            direct_node_addresses: vec![],
            network_id: DEFAULT_NETWORK_ID,
            private_key: None,
            relay: None,
        }
    }
}
