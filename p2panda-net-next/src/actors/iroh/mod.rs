// SPDX-License-Identifier: MIT OR Apache-2.0

mod connection;
mod endpoint;
#[cfg(test)]
mod tests;

use iroh::protocol::{DynProtocolHandler, ProtocolHandler};
use ractor::{ActorRef, call, registry};
use thiserror::Error;

use crate::actors::address_book::{ADDRESS_BOOK, ToAddressBook};
use crate::actors::iroh::connection::ConnectionActorError;
use crate::actors::{ActorNamespace, with_namespace};
use crate::addrs::NodeId;

pub use endpoint::{IROH_ENDPOINT, IrohEndpoint, ToIrohEndpoint};

pub fn register_protocol<P>(
    protocol_id: impl AsRef<[u8]>,
    handler: P,
    actor_namespace: ActorNamespace,
) -> Result<(), RegisterProtocolError>
where
    P: ProtocolHandler,
{
    let Some(actor) = registry::where_is(with_namespace(IROH_ENDPOINT, &actor_namespace)) else {
        return Err(RegisterProtocolError::ActorNotAvailable);
    };

    if ActorRef::<ToIrohEndpoint>::from(actor)
        .cast(ToIrohEndpoint::RegisterProtocol(
            protocol_id.as_ref().to_vec(),
            Box::new(handler),
        ))
        .is_err()
    {
        return Err(RegisterProtocolError::RegistrationFailed);
    }

    Ok(())
}

#[derive(Debug, Error)]
pub enum RegisterProtocolError {
    #[error("iroh endpoint actor is not available to register protocol handler")]
    ActorNotAvailable,

    #[error("could not register protocol in iroh endpoint")]
    RegistrationFailed,
}

pub async fn connect<T>(
    node_id: NodeId,
    protocol_id: impl AsRef<[u8]>,
    actor_namespace: ActorNamespace,
) -> Result<iroh::endpoint::Connection, ConnectError>
where
    T: Send + 'static,
{
    // Ask address book for available node information.
    let Some(address_book) = registry::where_is(with_namespace(ADDRESS_BOOK, &actor_namespace))
    else {
        return Err(ConnectError::ActorNotAvailable(ADDRESS_BOOK.into()));
    };
    let Some(node_info) = call!(
        ActorRef::<ToAddressBook<T>>::from(address_book),
        ToAddressBook::NodeInfo,
        node_id
    )
    .map_err(|_| ConnectError::ActorNotResponsive(ADDRESS_BOOK.into()))?
    else {
        return Err(ConnectError::NoAddressInfo(node_id));
    };

    // Check if node info contains address information for iroh transport.
    let endpoint_addr = iroh::EndpointAddr::try_from(node_info)
        .map_err(|_| ConnectError::NoAddressInfo(node_id))?;

    // Connect with iroh.
    let Some(actor) = registry::where_is(with_namespace(IROH_ENDPOINT, &actor_namespace)) else {
        return Err(ConnectError::ActorNotAvailable(IROH_ENDPOINT.into()));
    };
    let result = call!(
        ActorRef::<ToIrohEndpoint>::from(actor),
        ToIrohEndpoint::Connect,
        endpoint_addr,
        protocol_id.as_ref().to_vec()
    )
    .map_err(|_| ConnectError::ActorNotResponsive(IROH_ENDPOINT.into()))?;
    Ok(result?)
}

#[derive(Debug, Error)]
pub enum ConnectError {
    #[error("actor '{0}' is not available")]
    ActorNotAvailable(String),

    #[error("actor '{0}' is not responding to call")]
    ActorNotResponsive(String),

    #[error("address book does not have any iroh address info for node id {0}")]
    NoAddressInfo(NodeId),

    #[error(transparent)]
    ConnectionActor(#[from] ConnectionActorError),
}
