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
