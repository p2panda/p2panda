// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error as StdError;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::net::{Ipv4Addr, Ipv6Addr};

use p2panda_core::{PrivateKey, PublicKey};
use p2panda_discovery::address_book::AddressBookStore;
use ractor::errors::SpawnErr;
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{ActorRef, call, registry};
use thiserror::Error;
use tokio::task::JoinHandle;

use crate::actors::stream::{STREAM, ToStream};
use crate::actors::supervisor::{SUPERVISOR, Supervisor};
use crate::actors::{ActorNamespace, generate_actor_namespace, with_namespace};
use crate::args::{ApplicationArguments, ArgsBuilder};
use crate::topic_streams::EphemeralStream;
use crate::{NetworkId, NodeId, NodeInfo, TopicId};

/// Builds an overlay network for eventually-consistent pub/sub.
///
/// Network separation is achieved using the network identifier (`NetworkId`). Nodes using the same
/// network identifier will gradually discover one another over the lifetime of the network.
pub struct NetworkBuilder {
    args: ApplicationArguments,
}

impl NetworkBuilder {
    /// Returns a new instance of `NetworkBuilder` with default values assigned for all fields.
    pub fn new(network_id: NetworkId) -> Self {
        Self {
            args: ArgsBuilder::new(network_id).build(),
        }
    }

    /// Sets or overwrites the local IP for IPv4 sockets.
    ///
    /// Default is 0.0.0.0 (`UNSPECIFIED`).
    pub fn bind_ip_v4(mut self, ip: Ipv4Addr) -> Self {
        self.args.iroh_config.bind_ip_v4 = ip;
        self
    }

    /// Sets or overwrites the local bind port for IPv4 sockets.
    ///
    /// Default is 2022.
    pub fn bind_port_v4(mut self, port: u16) -> Self {
        self.args.iroh_config.bind_port_v4 = port;
        self
    }

    /// Sets or overwrites the local IP for IPv6 sockets.
    ///
    /// Default is :: (`UNSPECIFIED`).
    pub fn bind_ip_v6(mut self, ip: Ipv6Addr) -> Self {
        self.args.iroh_config.bind_ip_v6 = ip;
        self
    }

    /// Sets or overwrites the local bind port for IPv6 sockets.
    ///
    /// Default is 2023.
    pub fn bind_port_v6(mut self, port: u16) -> Self {
        self.args.iroh_config.bind_port_v6 = port;
        self
    }

    /// Sets or overwrites the private key.
    ///
    /// If this value is not set, the `NetworkBuilder` will generate a new, random key when
    /// building the network.
    pub fn private_key(mut self, private_key: PrivateKey) -> Self {
        self.args.public_key = private_key.public_key();
        self.args.private_key = private_key;
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
    pub fn relay(mut self, url: iroh::RelayUrl) -> Self {
        self.args.iroh_config.relay_urls.push(url);
        self
    }

    /// Returns a handle to a newly-spawned instance of `Network`.
    pub async fn build<S, T>(self, store: S) -> Result<Network, NetworkError>
    where
        S: AddressBookStore<T, NodeId, NodeInfo> + Clone + Debug + Send + Sync + 'static,
        S::Error: StdError + Send + Sync + 'static,
        T: Debug + Send + 'static,
    {
        // Compute a six character actor namespace using the node's public key.
        let actor_namespace = generate_actor_namespace(&self.args.public_key);

        // Spawn the root-level supervisor actor.
        let root_thread_pool = self.args.root_thread_pool.clone();
        let (supervisor_actor, supervisor_actor_handle) = Supervisor::spawn(
            Some(with_namespace(SUPERVISOR, &actor_namespace)),
            (self.args, store),
            root_thread_pool.clone(),
        )
        .await?;

        Ok(Network {
            actor_namespace,
            supervisor_actor,
            supervisor_actor_handle,
            root_thread_pool,
        })
    }
}

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("failed to create topic stream")]
    StreamCreation,

    #[error("failed to call {0} actor; it may be in the process of restarting")]
    ActorNotFound(String),

    #[error(transparent)]
    ActorSpawnError(#[from] SpawnErr),
}

#[derive(Debug)]
pub struct Network {
    actor_namespace: ActorNamespace,
    supervisor_actor: ActorRef<()>,
    supervisor_actor_handle: JoinHandle<()>,
    root_thread_pool: ThreadLocalActorSpawner,
}

impl Network {
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
    ) -> Result<EphemeralStream, NetworkError> {
        // Get a reference to the stream actor.
        if let Some(stream_actor) =
            registry::where_is(with_namespace(STREAM, &self.actor_namespace))
        {
            let actor: ActorRef<ToStream> = stream_actor.into();

            // Ask the stream actor for an ephemeral stream.
            let stream = call!(actor, ToStream::CreateEphemeralStream, *topic_id)
                .map_err(|_| NetworkError::StreamCreation)?;

            Ok(stream)
        } else {
            Err(NetworkError::ActorNotFound(STREAM.to_string()))
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
        bytes: Vec<u8>,
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

    pub(crate) fn message(bytes: Vec<u8>, delivered_from: PublicKey) -> Self {
        Self::Message {
            bytes,
            delivered_from,
        }
    }
}
