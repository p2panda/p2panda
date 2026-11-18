// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;

use crate::addrs::TrustedTransportInfo;
use crate::discovery::DiscoveryConfig;
use crate::gossip::GossipConfig;
use crate::iroh_endpoint::{IrohConfig, RelayUrl};
use crate::iroh_network::mdns::MdnsDiscoveryMode;
use crate::{DEFAULT_NETWORK_ID, NetworkId, NodeId};

/// Configuration parameters for the [`IrohNetwork`].
///
/// Note that [`MdnsDiscovery`] is active by default.
#[derive(Clone, Debug)]
pub struct IrohNetworkConfig {
    pub network_id: NetworkId,
    pub relay_urls: HashSet<RelayUrl>,
    pub bootstraps: HashSet<(NodeId, TrustedTransportInfo)>,
    pub mdns_mode: MdnsDiscoveryMode,
    pub discovery: DiscoveryConfig,
    pub gossip: GossipConfig,
    pub iroh: IrohConfig,
}

impl Default for IrohNetworkConfig {
    fn default() -> Self {
        Self {
            network_id: DEFAULT_NETWORK_ID,
            relay_urls: HashSet::new(),
            bootstraps: HashSet::new(),
            mdns_mode: MdnsDiscoveryMode::Active,
            discovery: DiscoveryConfig::default(),
            gossip: GossipConfig::default(),
            iroh: IrohConfig::default(),
        }
    }
}
