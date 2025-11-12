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
