// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;

use iroh::protocol::ProtocolHandler;
use ractor::{ActorRef, call, cast};
use thiserror::Error;
use tokio::sync::RwLock;

use crate::address_book::AddressBook;
use crate::iroh_endpoint::Builder;
use crate::iroh_endpoint::actors::{ConnectError, IrohEndpointArgs, ToIrohEndpoint};
use crate::{NetworkId, NodeId};

/// Establish encrypted, direct connections over Internet Protocol with QUIC.
///
/// ## Example
///
/// ```rust
/// # use std::error::Error;
/// #
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn Error>> {
/// # use p2panda_net::{AddressBook, Endpoint};
/// #
/// # let address_book = AddressBook::builder().spawn().await?;
/// #
/// // Generate Ed25519 key which will be used to authenticate node.
/// let private_key = p2panda_core::PrivateKey::new();
///
/// // Use this iroh relay as a "home relay".
/// let relay_url = "https://my.relay.org".parse().expect("valid relay url");
///
/// // Initialise endpoint with custom network identifier.
/// let endpoint = Endpoint::builder(address_book)
///     .network_id([1; 32])
///     .private_key(private_key)
///     .relay_url(relay_url)
///     .spawn()
///     .await?;
///
/// // Other nodes can use this id now to establish a direct connection.
/// println!("my node id: {}", endpoint.node_id());
/// #
/// # Ok(())
/// # }
/// ```
///
/// ## iroh
///
/// Most of the lower-level Internet Protocol networking is made possible by the work of [iroh]
/// utilising well-established and known standards, like QUIC for transport, (self-certified) TLS
/// 1.3 for transport encryption, QUIC Address Discovery (QAD) for STUN, TURN servers for relayed
/// fallbacks.
///
/// ## Network identifier
///
/// Use [`NetworkId`](crate::NetworkId) to actively partition the network. The identifier serves as
/// a shared secret; nodes will not be able to establish connections if their identifiers differ.
///
/// ## Custom Protocol Handlers
///
/// Register your own custom protocols using the [`Endpoint::accept`] method.
///
/// ## Relays
///
/// Use [`Builder::relay_url`] to register one or more iroh relay urls which are required to aid
/// in establishing a direct connection.
///
/// ## Resolving transport infos
///
/// To connect to any endpoint by it's node id / public key we first need to resolve it to the
/// associated addressing information (relay url, IPv4 and IPv6 addresses) before attempting to
/// establish a direct connection.
///
/// `Endpoint` takes the [`AddressBook`](crate::AddressBook) as a dependency which provides it with
/// the resolved transport information.
///
/// The address book itself is populated with resolved transport information by two services:
///
/// 1. [`MdnsDiscovery`](crate::MdnsDiscovery): Resolve addresses of nearby devices on the
///    local-area network.
/// 2. [`Discovery`](crate::Discovery): Resolve addresses using random-walk strategy, exploring the
///    network.
///
/// [iroh]: https://www.iroh.computer/
#[derive(Clone)]
pub struct Endpoint {
    pub(super) args: IrohEndpointArgs,
    pub(super) inner: Arc<RwLock<Inner>>,
}

pub(super) struct Inner {
    pub(super) actor_ref: Option<ActorRef<ToIrohEndpoint>>,
}

impl Endpoint {
    pub(crate) fn new(actor_ref: Option<ActorRef<ToIrohEndpoint>>, args: IrohEndpointArgs) -> Self {
        Self {
            args,
            inner: Arc::new(RwLock::new(Inner { actor_ref })),
        }
    }

    pub fn builder(address_book: AddressBook) -> Builder {
        Builder::new(address_book)
    }

    /// Return the internal iroh endpoint instance.
    pub async fn endpoint(&self) -> Result<iroh::Endpoint, EndpointError> {
        let inner = self.inner.read().await;
        let result = call!(
            inner.actor_ref.as_ref().expect("actor spawned in builder"),
            ToIrohEndpoint::Endpoint
        )
        .map_err(Box::new)?;
        Ok(result)
    }

    pub fn network_id(&self) -> NetworkId {
        self.args.0
    }

    pub fn node_id(&self) -> NodeId {
        self.args.1.public_key()
    }

    /// Register protocol handler for a given ALPN (protocol identifier).
    pub async fn accept<P: ProtocolHandler>(
        &self,
        protocol_id: impl AsRef<[u8]>,
        protocol_handler: P,
    ) -> Result<(), EndpointError> {
        let protocol_id = protocol_id.as_ref().to_vec();
        let inner = self.inner.read().await;
        cast!(
            inner.actor_ref.as_ref().expect("actor spawned in builder"),
            ToIrohEndpoint::RegisterProtocol(protocol_id, Box::new(protocol_handler))
        )
        .map_err(Box::new)?;
        Ok(())
    }

    /// Starts a connection attempt to a remote iroh endpoint and returns a future which can be
    /// awaited for establishing the final connection.
    ///
    /// The ALPN byte string, or application-level protocol identifier, is also required. The
    /// remote endpoint must support this alpn, otherwise the connection attempt will fail with an
    /// error.
    pub async fn connect(
        &self,
        node_id: NodeId,
        protocol_id: impl AsRef<[u8]>,
    ) -> Result<iroh::endpoint::Connection, EndpointError> {
        let inner = self.inner.read().await;
        let result = call!(
            inner.actor_ref.as_ref().expect("actor spawned in builder"),
            ToIrohEndpoint::Connect,
            node_id,
            protocol_id.as_ref().to_vec(),
            None
        )
        .map_err(Box::new)??;
        Ok(result)
    }

    pub async fn connect_with_config(
        &self,
        node_id: NodeId,
        protocol_id: impl AsRef<[u8]>,
        transport_config: Arc<iroh::endpoint::TransportConfig>,
    ) -> Result<iroh::endpoint::Connection, EndpointError> {
        let inner = self.inner.read().await;
        let result = call!(
            inner.actor_ref.as_ref().expect("actor spawned in builder"),
            ToIrohEndpoint::Connect,
            node_id,
            protocol_id.as_ref().to_vec(),
            Some(transport_config)
        )
        .map_err(Box::new)??;
        Ok(result)
    }
}

#[derive(Debug, Error)]
pub enum EndpointError {
    /// Spawning the internal actor failed.
    #[error(transparent)]
    ActorSpawn(#[from] ractor::SpawnErr),

    /// Spawning the internal actor as a child actor of a supervisor failed.
    #[cfg(feature = "supervisor")]
    #[error(transparent)]
    ActorLinkedSpawn(#[from] crate::supervisor::SupervisorError),

    /// Messaging with internal actor via RPC failed.
    #[error(transparent)]
    ActorRpc(#[from] Box<ractor::RactorErr<ToIrohEndpoint>>),

    #[error(transparent)]
    Connect(#[from] ConnectError),
}

impl Drop for Inner {
    fn drop(&mut self) {
        if let Some(actor_ref) = self.actor_ref.take() {
            actor_ref.stop(None);
        }
    }
}
