// SPDX-License-Identifier: MIT OR Apache-2.0

use std::net::{Ipv4Addr, Ipv6Addr};

/// Default port of a node socket.
pub const DEFAULT_BIND_PORT: u16 = 2022;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IrohConfig {
    pub bind_ip_v4: Ipv4Addr,
    pub bind_port_v4: u16,
    pub bind_ip_v6: Ipv6Addr,
    pub bind_port_v6: u16,
    pub relay_urls: Vec<iroh::RelayUrl>,
}

impl Default for IrohConfig {
    fn default() -> Self {
        Self {
            bind_ip_v4: Ipv4Addr::UNSPECIFIED,
            bind_port_v4: DEFAULT_BIND_PORT,
            bind_ip_v6: Ipv6Addr::UNSPECIFIED,
            bind_port_v6: DEFAULT_BIND_PORT + 1,
            relay_urls: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct DiscoveryConfig {
    /// Number of random walkers which "explore" the network at the same time.
    pub random_walkers_count: usize,

    /// Probability of resetting the random walk and starting from scratch, determined on every
    /// walking step.
    ///
    /// ```text
    /// 0.0 = Never reset
    /// 1.0 = Always reset
    /// ```
    ///
    /// Defaults to 0.02 (2%) probability.
    pub reset_walk_probability: f64,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            random_walkers_count: 1,
            reset_walk_probability: 0.02, // 2% chance
        }
    }
}
