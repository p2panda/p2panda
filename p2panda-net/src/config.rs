// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{NetworkId, NodeAddress, RelayUrl};

/// Default port of a node socket.
pub const DEFAULT_BIND_PORT: u16 = 2022;

/// Default network id.
pub const DEFAULT_NETWORK_ID: NetworkId = [0; 32];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub bind_port: u16,
    pub direct_node_addresses: Vec<NodeAddress>,
    pub network_id: NetworkId,
    pub private_key: Option<PathBuf>,
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
