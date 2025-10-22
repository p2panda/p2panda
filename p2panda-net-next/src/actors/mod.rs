// SPDX-License-Identifier: MIT OR Apache-2.0

use iroh::NodeAddr;
use iroh::endpoint::{ConnectWithOptsError, Connecting};
use iroh::protocol::ProtocolHandler;
use ractor::{ActorRef, call, registry};
use thiserror::Error;

use crate::NodeId;
use crate::actors::address_book::{ADDRESS_BOOK, ToAddressBook};
use crate::actors::endpoint::iroh::{IROH_TRANSPORT, ToIroh};
use crate::actors::endpoint::router::{IROH_ROUTER, ToIrohRouter};

mod address_book;
pub mod discovery;
pub mod endpoint;
mod events;
mod gossip;
pub mod network;
mod subscription;
pub mod supervisor;
mod sync;
#[cfg(test)]
mod test_utils;

pub fn register_protocol<P>(
    protocol_id: impl AsRef<[u8]>,
    handler: P,
) -> Result<(), RegisterProtocolError>
where
    P: ProtocolHandler,
{
    let Some(router) = registry::where_is(IROH_ROUTER.into()) else {
        return Err(RegisterProtocolError::RouterNotAvailable);
    };

    if let Err(_) = ActorRef::<ToIrohRouter>::from(router).cast(ToIrohRouter::RegisterProtocol(
        protocol_id.as_ref().to_vec(),
        Box::new(handler),
    )) {
        return Err(RegisterProtocolError::RegistrationFailed);
    }

    Ok(())
}

#[derive(Debug, Error)]
pub enum RegisterProtocolError {
    #[error("iroh router actor is not available to register protocol handler")]
    RouterNotAvailable,

    #[error("could not register protocol in router")]
    RegistrationFailed,
}

pub async fn connect<T>(
    node_id: NodeId,
    protocol_id: impl AsRef<[u8]>,
) -> Result<Connecting, ConnectError>
where
    T: Send + 'static,
{
    // Ask address book for available node information.
    let Some(address_book) = registry::where_is(ADDRESS_BOOK.into()) else {
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
    let node_addr =
        NodeAddr::try_from(node_info).map_err(|_| ConnectError::NoAddressInfo(node_id))?;

    // Connect with iroh.
    let Some(actor) = registry::where_is(IROH_TRANSPORT.into()) else {
        return Err(ConnectError::ActorNotAvailable(IROH_TRANSPORT.into()));
    };

    let result = call!(
        ActorRef::<ToIroh>::from(actor),
        ToIroh::Connect,
        node_addr,
        protocol_id.as_ref().to_vec()
    )
    .map_err(|_| ConnectError::ActorNotResponsive(IROH_TRANSPORT.into()))?;

    Ok(result?)
}

#[derive(Debug, Error)]
pub enum ConnectError {
    #[error("actor '{0}' is not available")]
    ActorNotAvailable(String),

    #[error("actor '{0}' is not responding to call")]
    ActorNotResponsive(String),

    #[error(transparent)]
    Iroh(#[from] ConnectWithOptsError),

    #[error("address book does not have any iroh address info for node id {0}")]
    NoAddressInfo(NodeId),
}
