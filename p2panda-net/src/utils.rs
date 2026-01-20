// SPDX-License-Identifier: MIT OR Apache-2.0

//! Formatting, conversion and connectivity status utilities.
use std::net::{IpAddr, SocketAddr};

use crate::NodeId;

/// Returns a displayable string representing the underlying value in a short format, easy to read
/// during debugging and logging.
pub trait ShortFormat {
    fn fmt_short(&self) -> String;
}

impl ShortFormat for NodeId {
    fn fmt_short(&self) -> String {
        self.to_hex()[0..10].to_string()
    }
}

impl ShortFormat for iroh::EndpointId {
    fn fmt_short(&self) -> String {
        self.to_string()[0..10].to_string()
    }
}

impl ShortFormat for [u8; 32] {
    fn fmt_short(&self) -> String {
        hex::encode(&self[0..5]).to_string()
    }
}

impl ShortFormat for Vec<u8> {
    fn fmt_short(&self) -> String {
        hex::encode(&self[0..5]).to_string()
    }
}

impl ShortFormat for Vec<NodeId> {
    fn fmt_short(&self) -> String {
        let list: Vec<String> = self.iter().map(|addr| addr.fmt_short()).collect();
        format!("[{}]", list.join(", "))
    }
}

impl ShortFormat for Vec<iroh::EndpointId> {
    fn fmt_short(&self) -> String {
        let list: Vec<String> = self
            .iter()
            .map(|addr| addr.fmt_short().to_string())
            .collect();
        format!("[{}]", list.join(", "))
    }
}

/// Connectivity status derived from a given IP address.
///
/// Defines whether a node appears to be locally-reachable (on the host machine or via WLAN) or
/// globally-reachable (via the internet). This is not a guarantee of the overall node connectivity
/// status but a best-guess based on the provided IP address.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConnectivityStatus {
    /// The IP address is neither link-local, loopback nor global.
    Other,

    /// The IP address is link-local or loopback.
    Local,

    /// The IP address appears to be globally reachable.
    Global,
}

impl ConnectivityStatus {
    pub fn is_global(&self) -> bool {
        self == &ConnectivityStatus::Global
    }
}

/// Parse a `SocketAddr` and return the best approximation of the connectivity status based on the
/// IP address.
pub fn connectivity_status(addr: &SocketAddr) -> ConnectivityStatus {
    match addr.ip() {
        IpAddr::V4(ip) => {
            if ip.is_loopback() || ip.is_link_local() || ip.is_private() {
                ConnectivityStatus::Local
            } else if ip.is_multicast()
                || ip.is_broadcast()
                || ip.is_documentation()
                || ip.is_unspecified()
            {
                ConnectivityStatus::Other
            } else {
                ConnectivityStatus::Global
            }
        }
        IpAddr::V6(ip) => {
            if ip.is_loopback() || ip.is_unique_local() || ip.is_unicast_link_local() {
                ConnectivityStatus::Local
            } else if ip.is_multicast() || ip.is_unspecified() {
                ConnectivityStatus::Other
            } else {
                ConnectivityStatus::Global
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ConnectivityStatus;

    #[test]
    fn order() {
        assert!(ConnectivityStatus::Global > ConnectivityStatus::Local);
        assert!(ConnectivityStatus::Local > ConnectivityStatus::Other);
    }
}
