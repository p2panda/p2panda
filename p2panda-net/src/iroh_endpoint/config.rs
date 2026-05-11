// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Display;
use std::net::{Ipv4Addr, Ipv6Addr};

/// Bluetooth Low-Energy (BLE) mode.
///
/// By default this is set to "disabled".
///
/// This default is chosen to prioritise privacy and security; only choose "active" mode only if
/// you can accept leaking your address and public key to untrusted, in-range BLE devices.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum BleMode {
    Active,
    #[default]
    Disabled,
}

impl Display for BleMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            BleMode::Active => "active",
            BleMode::Disabled => "disabled",
        };
        write!(f, "{value}")
    }
}

impl BleMode {
    pub fn is_active(&self) -> bool {
        self == &BleMode::Active
    }
}

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
}

impl Default for IrohConfig {
    fn default() -> Self {
        Self {
            bind_ip_v4: Ipv4Addr::UNSPECIFIED,
            bind_port_v4: 0,
            bind_ip_v6: Ipv6Addr::UNSPECIFIED,
            bind_port_v6: 0,
        }
    }
}
