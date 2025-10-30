// SPDX-License-Identifier: MIT OR Apache-2.0

use std::net::{Ipv4Addr, Ipv6Addr};

use iroh::protocol::DynProtocolHandler as ProtocolHandler;
use p2panda_core::{PrivateKey, PublicKey};
use ractor::errors::SpawnErr;
use ractor::{Actor, ActorRef, call, registry};
use thiserror::Error;
use tokio::task::JoinHandle;

use crate::actors::subscription::ToSubscription;
use crate::actors::supervisor::{NetworkConfig, Supervisor};
use crate::addrs::RelayUrl;
use crate::protocols::{self, ProtocolId};
use crate::topic_streams::EphemeralTopicStream;
use crate::utils::with_suffix;
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

    /// Returns a handle to a newly-spawned instance of `Network`.
    pub async fn build(mut self) -> Result<Network, NetworkError> {
        let private_key = self.private_key.unwrap_or_default();

        // Compute the six character public key suffix.
        //
        // The suffix is used to create actor names which are unique to the node.
        let public_key_suffix = &private_key.public_key().to_hex()[..6];

        // Spawn the root-level supervisor actor.
        let (supervisor_actor, supervisor_actor_handle) = Actor::spawn(
            Some(with_suffix("supervisor", public_key_suffix)),
            Supervisor,
            (private_key, self.network_config),
        )
        .await?;

        let network = Network::new(
            public_key_suffix.to_string(),
            supervisor_actor,
            supervisor_actor_handle,
        );

        Ok(network)
    }
}

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("failed to create topic stream")]
    StreamCreation,

    #[error(transparent)]
    ActorSpawnError(#[from] SpawnErr),
}

#[derive(Debug)]
pub struct Network {
    public_key_suffix: String,
    supervisor_actor: ActorRef<()>,
    supervisor_actor_handle: JoinHandle<()>,
}

impl Network {
    fn new(
        public_key_suffix: String,
        supervisor_actor: ActorRef<()>,
        supervisor_actor_handle: JoinHandle<()>,
    ) -> Self {
        Self {
            public_key_suffix,
            supervisor_actor,
            supervisor_actor_handle,
        }
    }

    /// Creates an ephemeral messaging stream and returns a handle.
    ///
    /// The returned handle can be used to publish ephemeral messages into the stream. These
    /// messages will be propagated to other nodes which share an interest in the topic ID.
    ///
    /// Calling `.subscribe()` on the handle returns an `EphemeralTopicStreamSubscription`; this
    /// acts as a receiver for messages authored by other nodes for the shared topic ID.
    ///
    /// Both the `EphemeralTopicStream` and `EphemeralTopicStreamSubscription` handles can be
    /// cloned. The subscription handle acts as a broadcast receiver, meaning that each clones of
    /// the receiver will receive every message. It is also possible to obtain multiple publishing
    /// handles by calling `ephemeral_stream()` repeatedly.
    pub async fn ephemeral_stream(
        &self,
        topic_id: &TopicId,
    ) -> Result<EphemeralTopicStream, NetworkError> {
        // Get a reference to the subscription actor.
        if let Some(subscription_actor) =
            registry::where_is(with_suffix("subscription", &self.public_key_suffix))
        {
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
pub type ToNetwork = Vec<u8>;

/// Message received from the network.
#[derive(Debug, Clone, PartialEq)]
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

impl FromNetwork {
    pub(crate) fn ephemeral_message(bytes: Vec<u8>, delivered_from: PublicKey) -> Self {
        Self::EphemeralMessage {
            bytes,
            delivered_from,
        }
    }

    pub(crate) fn message(
        header: Vec<u8>,
        payload: Option<Vec<u8>>,
        delivered_from: PublicKey,
    ) -> Self {
        Self::Message {
            header,
            payload,
            delivered_from,
        }
    }
}
