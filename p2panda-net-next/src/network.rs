// SPDX-License-Identifier: MIT OR Apache-2.0

use std::net::{Ipv4Addr, Ipv6Addr};

use iroh::protocol::DynProtocolHandler as ProtocolHandler;
use p2panda_core::{PrivateKey, PublicKey};
use ractor::{call, registry, ActorRef};
use thiserror::Error;

use crate::actors::network::NetworkConfig;
use crate::actors::subscription::ToSubscription;
use crate::addrs::RelayUrl;
use crate::protocols::{self, ProtocolId};
use crate::topic_streams::EphemeralTopicStream;
use crate::{NetworkId, TopicId};

/// Builds an overlay network for eventually-consistent pub/sub.
///
/// Network separation is achieved using the network identifier (`NetworkId`). Nodes using the same
/// network identifier will gradually discover one another over the lifetime of the network.
#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct NetworkBuilder {
    network_id: NetworkId,
    network_config: NetworkConfig,
    private_key: Option<PrivateKey>,
}

impl NetworkBuilder {
    /// Returns a new instance of `NetworkBuilder` with default values assigned for all fields.
    pub fn new(network_id: NetworkId) -> Self {
        Self {
            network_id,
            network_config: NetworkConfig::default(),
            private_key: None,
        }
    }

    /// Sets or overwrites the local IP for IPv4 sockets.
    ///
    /// Default is 0.0.0.0 (`UNSPECIFIED`).
    pub fn bind_ip_v4(mut self, ip: Ipv4Addr) -> Self {
        self.network_config.endpoint_config.bind_ip_v4 = ip;
        self
    }

    /// Sets or overwrites the local bind port for IPv4 sockets.
    ///
    /// Default is 2022.
    pub fn bind_port_v4(mut self, port: u16) -> Self {
        self.network_config.endpoint_config.bind_port_v4 = port;
        self
    }

    /// Sets or overwrites the local IP for IPv6 sockets.
    ///
    /// Default is :: (`UNSPECIFIED`).
    pub fn bind_ip_v6(mut self, ip: Ipv6Addr) -> Self {
        self.network_config.endpoint_config.bind_ip_v6 = ip;
        self
    }

    /// Sets or overwrites the local bind port for IPv6 sockets.
    ///
    /// Default is 2023.
    pub fn bind_port_v6(mut self, port: u16) -> Self {
        self.network_config.endpoint_config.bind_port_v6 = port;
        self
    }

    /// Sets or overwrites the private key.
    ///
    /// If this value is not set, the `NetworkBuilder` will generate a new, random key when
    /// building the network.
    pub fn private_key(mut self, private_key: PrivateKey) -> Self {
        self.private_key = Some(private_key);
        self
    }

    /// Adds a custom protocol for communication between two peers.
    fn _protocol(mut self, id: &ProtocolId, handler: impl ProtocolHandler) -> Self {
        // Hash the protocol ID with the network ID.
        //
        // The hashed ID is what will be registered with the iroh `Endpoint`.
        let identifier_hash = protocols::hash_protocol_id_with_network_id(id, &self.network_id);

        self.network_config
            .endpoint_config
            .protocols
            .insert(identifier_hash, Box::new(handler));
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
    pub fn relay(mut self, url: RelayUrl) -> Self {
        self.network_config.endpoint_config.relays.push(url);
        self
    }
}

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("failed to create topic stream")]
    StreamCreation,
}

#[derive(Debug)]
pub struct Network;

impl Network {
    pub async fn stream(topic: T, live_mode: bool) -> TopicStream {
        todo!()
    }

    pub async fn ephemeral_stream(
        topic_id: &TopicId,
    ) -> Result<EphemeralTopicStream, NetworkError> {
        // Get a reference to the subscription actor.
        if let Some(subscription_actor) = registry::where_is("subscription".to_string()) {
            let actor: ActorRef<ToSubscription> = subscription_actor.into();

            // Ask the subscription actor for an ephemeral stream.
            let stream = call!(actor, ToSubscription::CreateEphemeralStream, *topic_id)
                .map_err(|_| NetworkError::StreamCreation)?;

            Ok(stream)
        } else {
            Err(NetworkError::StreamCreation)
        }
    }
}

/// Bytes to be sent into the network.
#[derive(Clone, Debug)]
// TODO: Consider turning this into `pub type ToNetwork = Vec<u8>`.
pub enum ToNetwork {
    Message { bytes: Vec<u8> },
}

/// Message received from the network.
pub enum FromNetwork {
    EphemeralMessage {
        bytes: Vec<u8>,
        delivered_from: PublicKey,
    },
    Message {
        header: Vec<u8>,
        payload: Option<Vec<u8>>,
        delivered_from: PublicKey,
    },
}
