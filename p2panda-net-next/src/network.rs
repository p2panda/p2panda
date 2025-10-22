// SPDX-License-Identifier: MIT OR Apache-2.0

use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::Arc;

use iroh::RelayUrl;
use p2panda_core::PrivateKey;
use ractor::concurrency::JoinHandle;
use ractor::{Actor, ActorRef, SpawnErr};
use thiserror::Error;

use crate::NetworkId;
use crate::actors::supervisor::{SUPERVISOR, Supervisor};
use crate::args::ApplicationArguments;
use crate::config::Config;
use crate::store::MemoryStore;

/// Builds an overlay network for eventually-consistent and ephemeral messaging pub/sub.
///
/// Network separation is achieved using the network identifier (`NetworkId`). Nodes using the same
/// network identifier will gradually discover one another over the lifetime of the network.
#[derive(Debug)]
pub struct NetworkBuilder {
    config: Config,
    private_key: Option<PrivateKey>,
}

impl NetworkBuilder {
    /// Returns a new instance of `NetworkBuilder` with default values assigned for all fields.
    pub fn new(network_id: NetworkId) -> Self {
        Self {
            config: Config::from_network_id(network_id),
            private_key: None,
        }
    }

    /// Sets or overwrites the local IP for IPv4 sockets.
    ///
    /// Default is 0.0.0.0 (`UNSPECIFIED`).
    pub fn bind_ip_v4(mut self, ip: Ipv4Addr) -> Self {
        self.config.iroh.bind_ip_v4 = ip;
        self
    }

    /// Sets or overwrites the local bind port for IPv4 sockets.
    ///
    /// Default is 2022.
    pub fn bind_port_v4(mut self, port: u16) -> Self {
        self.config.iroh.bind_port_v4 = port;
        self
    }

    /// Sets or overwrites the local IP for IPv6 sockets.
    ///
    /// Default is :: (`UNSPECIFIED`).
    pub fn bind_ip_v6(mut self, ip: Ipv6Addr) -> Self {
        self.config.iroh.bind_ip_v6 = ip;
        self
    }

    /// Sets or overwrites the local bind port for IPv6 sockets.
    ///
    /// Default is 2023.
    pub fn bind_port_v6(mut self, port: u16) -> Self {
        self.config.iroh.bind_port_v6 = port;
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

    /// Sets a relay used by the local network to facilitate the establishment of direct
    /// connections. Multiple relays can be added.
    ///
    /// Relay nodes are STUN servers which help in establishing a peer-to-peer connection if one or
    /// both of the peers are behind a NAT. The relay node might offer proxy functionality on top
    /// (via the Tailscale DERP protocol which is very similar to TURN) if the connection attempt
    /// fails, which will serve to relay the data in that case.
    // TODO: Expose QUIC address discovery address as `Option<u16>` or config struct.
    pub fn relay(mut self, url: RelayUrl) -> Self {
        self.config.iroh.relay_urls.push(url);
        self
    }

    pub async fn build<T>(self) -> Result<Network, NetworkError>
    where
        T: Send + Sync + 'static,
    {
        let args = ApplicationArguments {
            config: self.config.clone(),
            private_key: self.private_key.unwrap_or_default(),
        };

        // @TODO: Make this configurable.
        let store = MemoryStore::new();

        let (actor, actor_handle) = {
            let supervisor = Supervisor::<_, T>::new(store);
            Actor::spawn(Some(SUPERVISOR.into()), supervisor, args.clone()).await?
        };

        let inner = Arc::new(NetworkInner {
            actor,
            actor_handle,
            args,
        });

        Ok(Network { inner })
    }
}

#[derive(Clone, Debug)]
pub struct Network {
    inner: Arc<NetworkInner>,
}

impl Network {}

#[derive(Debug)]
struct NetworkInner {
    actor: ActorRef<()>,
    actor_handle: JoinHandle<()>,
    args: ApplicationArguments,
}

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error(transparent)]
    ActorSpawn(#[from] SpawnErr),
}
