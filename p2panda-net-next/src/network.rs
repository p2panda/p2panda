// SPDX-License-Identifier: MIT OR Apache-2.0

use std::net::{Ipv4Addr, Ipv6Addr};

use iroh::protocol::DynProtocolHandler as ProtocolHandler;
use p2panda_core::PrivateKey;

use crate::NetworkId;
use crate::actors::network::NetworkConfig;
use crate::addrs::RelayUrl;
use crate::protocols::ProtocolId;
use crate::protocols::{self};

#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct NetworkBuilder {
    network_id: NetworkId,
    network_config: NetworkConfig,
}

impl NetworkBuilder {
    /// Returns a new instance of `NetworkBuilder` with default values assigned for all fields.
    pub fn new(network_id: NetworkId) -> Self {
        Self {
            network_id,
            network_config: NetworkConfig::default(),
        }
    }

    /// Sets or overwrites the local IP for IPv4 sockets.
    ///
    /// Default is 0.0.0.0 (`UNSPECIFIED`).
    pub fn _bind_ip_v4(mut self, ip: Ipv4Addr) -> Self {
        self.network_config.endpoint_config.bind_ip_v4 = ip;
        self
    }

    /// Sets or overwrites the local bind port for IPv4 sockets.
    ///
    /// Default is 2022.
    pub fn _bind_port_v4(mut self, port: u16) -> Self {
        self.network_config.endpoint_config.bind_port_v4 = port;
        self
    }

    /// Sets or overwrites the local IP for IPv6 sockets.
    ///
    /// Default is :: (`UNSPECIFIED`).
    pub fn _bind_ip_v6(mut self, ip: Ipv6Addr) -> Self {
        self.network_config.endpoint_config.bind_ip_v6 = ip;
        self
    }

    /// Sets or overwrites the local bind port for IPv6 sockets.
    ///
    /// Default is 2023.
    pub fn _bind_port_v6(mut self, port: u16) -> Self {
        self.network_config.endpoint_config.bind_port_v6 = port;
        self
    }

    /// Sets or overwrites the private key.
    ///
    /// If this value is not set, the `NetworkBuilder` will generate a new, random key when
    /// building the network.
    pub fn _private_key(mut self, private_key: PrivateKey) -> Self {
        self.network_config.endpoint_config.private_key = private_key;
        self
    }

    /// Adds a custom protocol for communication between two peers.
    pub fn _protocol(mut self, id: &ProtocolId, handler: impl ProtocolHandler) -> Self {
        // XOR the protocol ID with the network ID.
        //
        // The XOR'd ID is what will be registered with the iroh `Endpoint`.
        let identifier_xor = protocols::protocol_id_xor(id, self.network_id);

        self.network_config
            .endpoint_config
            .protocols
            .insert(identifier_xor, Box::new(handler));
        self
    }

    /// Sets a relay used by the local network to facilitate the establishment of direct
    /// connections. Multiple relays can be added.
    ///
    /// Relay nodes are STUN servers which help in establishing a peer-to-peer connection if one or
    /// both of the peers are behind a NAT. The relay node might offer proxy functionality on top
    /// (via the Tailscale DERP protocol which is very similar to TURN) if the connection attempt
    /// fails, which will serve to relay the data in that case.
    // TODO: Expose QUIC address discovery address as `Option<u16>` or config struct.
    pub fn _relay(mut self, url: RelayUrl) -> Self {
        self.network_config.endpoint_config.relays.push(url);
        self
    }
}
