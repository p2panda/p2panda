// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;

use iroh::protocol::ProtocolHandler;
use ractor::{ActorRef, call, cast};
use thiserror::Error;
use tokio::sync::RwLock;

use crate::NodeId;
use crate::address_book::AddressBook;
use crate::iroh::Builder;
use crate::iroh::actors::{ConnectError, ToIrohEndpoint};

#[derive(Clone)]
pub struct Endpoint {
    pub(crate) actor_ref: Arc<RwLock<ActorRef<ToIrohEndpoint>>>,
}

impl Endpoint {
    pub fn builder(address_book: AddressBook) -> Builder {
        Builder::new(address_book)
    }

    /// Return the internal iroh endpoint instance.
    pub async fn endpoint(&self) -> Result<iroh::Endpoint, EndpointError> {
        let actor_ref = self.actor_ref.read().await;
        let result = call!(actor_ref, ToIrohEndpoint::Endpoint)?;
        Ok(result)
    }

    /// Register protocol handler for a given ALPN (protocol identifier).
    pub async fn accept<P: ProtocolHandler>(
        &self,
        protocol_id: impl AsRef<[u8]>,
        protocol_handler: P,
    ) -> Result<(), EndpointError> {
        let protocol_id = protocol_id.as_ref().to_vec();
        let actor_ref = self.actor_ref.read().await;
        cast!(
            actor_ref,
            ToIrohEndpoint::RegisterProtocol(protocol_id, Box::new(protocol_handler))
        )?;
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
        self.connect_with_config(node_id, protocol_id, None).await
    }

    pub async fn connect_with_config(
        &self,
        node_id: NodeId,
        protocol_id: impl AsRef<[u8]>,
        transport_config: Option<Arc<iroh::endpoint::TransportConfig>>,
    ) -> Result<iroh::endpoint::Connection, EndpointError> {
        let protocol_id = protocol_id.as_ref().to_vec();
        let actor_ref = self.actor_ref.read().await;
        let result = call!(
            actor_ref,
            ToIrohEndpoint::Connect,
            node_id,
            protocol_id,
            transport_config
        )??;
        Ok(result)
    }
}

#[derive(Debug, Error)]
pub enum EndpointError {
    /// Spawning the internal actor failed.
    #[error(transparent)]
    ActorSpawn(#[from] ractor::SpawnErr),

    /// Messaging with internal actor via RPC failed.
    #[error(transparent)]
    ActorRpc(#[from] ractor::RactorErr<ToIrohEndpoint>),

    #[error(transparent)]
    Connect(#[from] ConnectError),
}
