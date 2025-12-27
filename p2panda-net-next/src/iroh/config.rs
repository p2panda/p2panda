// SPDX-License-Identifier: MIT OR Apache-2.0

#[cfg(feature = "mdns")]
use std::fmt::Display;
use std::net::{Ipv4Addr, Ipv6Addr};

/// Default port of a node socket.
pub const DEFAULT_BIND_PORT: u16 = 2022;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IrohConfig {
    /// IPv4 address to bind to.
    pub bind_ip_v4: Ipv4Addr,

    /// Port used for IPv4 socket address.
    ///
    /// Setting the port to `0` will use a random port. If the port specified is already in use, it
    /// will fallback to choosing a random port.
    pub bind_port_v4: u16,

    /// IPv6 address to bind to.
    pub bind_ip_v6: Ipv6Addr,

    /// Port used for IPv6 socket address.
    ///
    /// Setting the port to `0` will use a random port. If the port specified is already in use, it
    /// will fallback to choosing a random port.
    pub bind_port_v6: u16,

    /// Sets the mDNS discovery mode.
    ///
    /// By default this is set to "passive" and we are not actively advertising our endpoint
    /// address to the local-area network.
    ///
    /// This default is chosen to prioritize privacy and security, choose "active" mode only if you
    /// can trust that leaking your address and public key on local-area networks is safe for the
    /// users.
    #[cfg(feature = "mdns")]
    pub mdns_discovery_mode: MdnsDiscoveryMode,

    /// Sets the relay servers to assist in establishing connectivity.
    ///
    /// Relay servers are used to establish initial connection with another iroh endpoint. They
    /// also perform various functions related to hole punching.
    pub relay_urls: Vec<iroh::RelayUrl>,
}

impl Default for IrohConfig {
    fn default() -> Self {
        Self {
            bind_ip_v4: Ipv4Addr::UNSPECIFIED,
            bind_port_v4: DEFAULT_BIND_PORT,
            bind_ip_v6: Ipv6Addr::UNSPECIFIED,
            bind_port_v6: DEFAULT_BIND_PORT + 2,
            #[cfg(feature = "mdns")]
            mdns_discovery_mode: MdnsDiscoveryMode::default(),
            relay_urls: Vec::new(),
        }
    }
}

#[cfg(feature = "mdns")]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum MdnsDiscoveryMode {
    Active,
    #[default]
    Passive,
    Disabled,
}

#[cfg(feature = "mdns")]
impl Display for MdnsDiscoveryMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            MdnsDiscoveryMode::Active => "active",
            MdnsDiscoveryMode::Passive => "passive",
            MdnsDiscoveryMode::Disabled => "disabled",
        };
        write!(f, "{value}")
    }
}

#[cfg(feature = "mdns")]
impl MdnsDiscoveryMode {
    pub fn is_active(&self) -> bool {
        self == &MdnsDiscoveryMode::Active
    }
}
