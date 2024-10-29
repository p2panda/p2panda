// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{NetworkId, NodeAddress, RelayUrl};

/// Default port of a node socket.
pub const DEFAULT_BIND_PORT: u16 = 2022;

/// Default network id.
pub const DEFAULT_NETWORK_ID: NetworkId = [0; 32];

/// Configuration parameters for the local network node.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    /// Bind port for the IPv4 socket. The IPv6 socket will be bound to `bind_port` + 1.
    pub bind_port: u16,

    /// Direct node addresses for the local node (e.g. "0.0.0.0:2026").
    pub direct_node_addresses: Vec<NodeAddress>,

    /// Identifier of the network to be joined.
    pub network_id: NetworkId,

    /// Path to the local private key. If not provided, a random keypair will be generated.
    pub private_key: Option<PathBuf>,

    /// Relay URL to help in establishing a peer-to-peer connection if one or both peers are behind
    /// a NAT.
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
