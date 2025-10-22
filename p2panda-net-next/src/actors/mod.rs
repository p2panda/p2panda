// SPDX-License-Identifier: MIT OR Apache-2.0

use iroh::{
    NodeAddr,
    endpoint::{ConnectWithOptsError, Connecting},
    protocol::ProtocolHandler,
};
use ractor::{ActorRef, RactorErr, call, registry};
use thiserror::Error;

use crate::{
    NodeId,
    actors::{
        address_book::{ADDRESS_BOOK, ToAddressBook},
        endpoint::{
            iroh::{IROH_TRANSPORT, ToIroh},
            router::{IROH_ROUTER, ToIrohRouter},
        },
    },
};

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
) -> Result<Connecting, ConnectError<T>>
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
    )?
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
    )?;

    Ok(result?)
}

#[derive(Debug, Error)]
pub enum ConnectError<T> {
    #[error("actor '{0}' is not available")]
    ActorNotAvailable(String),

    #[error(transparent)]
    IrohActor(#[from] RactorErr<ToIroh>),

    #[error(transparent)]
    AddressBookActor(#[from] RactorErr<ToAddressBook<T>>),

    #[error(transparent)]
    Iroh(#[from] ConnectWithOptsError),

    #[error("address book does not have any iroh address info for node id {0}")]
    NoAddressInfo(NodeId),
}
