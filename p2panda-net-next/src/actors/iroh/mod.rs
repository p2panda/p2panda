// SPDX-License-Identifier: MIT OR Apache-2.0

mod connection;
mod endpoint;
#[cfg(feature = "mdns")]
mod mdns;
#[cfg(test)]
mod tests;

use std::num::ParseIntError;
use std::str::FromStr;

use iroh::discovery::UserData;
use iroh::endpoint_info::MaxLengthExceededError;
use iroh::protocol::ProtocolHandler;
use p2panda_core::{IdentityError, Signature};
use ractor::{ActorRef, call, registry};
use thiserror::Error;

use crate::TransportInfo;
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

pub async fn connect(
    node_id: NodeId,
    protocol_id: impl AsRef<[u8]>,
    actor_namespace: ActorNamespace,
) -> Result<iroh::endpoint::Connection, ConnectError> {
    // Ask address book for available node information.
    let Some(address_book) = registry::where_is(with_namespace(ADDRESS_BOOK, &actor_namespace))
    else {
        return Err(ConnectError::ActorNotAvailable(ADDRESS_BOOK.into()));
    };
    let Some(node_info) = call!(
        ActorRef::<ToAddressBook>::from(address_book),
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

/// Helper to bring additional transport info (signature and timestamp) into iroh's user data
/// struct.
///
/// We need this data to check the authenticity of the transport info.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserDataTransportInfo {
    pub signature: Signature,
    pub timestamp: u64,
}

impl UserDataTransportInfo {
    pub fn from_transport_info(info: TransportInfo) -> Self {
        Self {
            signature: info.signature,
            timestamp: info.timestamp,
        }
    }
}

impl TryFrom<TransportInfo> for UserData {
    type Error = MaxLengthExceededError;

    fn try_from(info: TransportInfo) -> Result<Self, Self::Error> {
        UserData::try_from(UserDataTransportInfo::from_transport_info(info))
    }
}

const INFO_SEPARATOR: char = '.';

impl TryFrom<UserDataTransportInfo> for UserData {
    type Error = MaxLengthExceededError;

    fn try_from(info: UserDataTransportInfo) -> Result<Self, Self::Error> {
        // Encode the signature as an hex-string (128 characters) and the timestamp as a plain
        // number. There's a 245 character limit for iroh's user data due to the limit of DNS TXT
        // records.
        //
        // NOTE: This will currently fail if the u64 integer gets too large .. we can't "remote
        // crash" nodes because of that at least.
        UserData::try_from(format!(
            "{}{INFO_SEPARATOR}{}",
            info.signature, info.timestamp
        ))
    }
}

impl TryFrom<UserData> for UserDataTransportInfo {
    type Error = TransportInfoTxtError;

    fn try_from(user_data: UserData) -> Result<Self, Self::Error> {
        let user_data = user_data.to_string();

        // Try to split string by separator into two halfs.
        let parts: Vec<_> = user_data.split(INFO_SEPARATOR).collect();
        if parts.len() != 2 {
            return Err(TransportInfoTxtError::Size(parts.len()));
        }

        // Try to parse halfs into signature and timestamp.
        let signature = Signature::from_str(parts.first().expect("we've checked the size before"))?;
        let timestamp = u64::from_str(parts.get(1).expect("we've checked the size before"))?;

        Ok(Self {
            signature,
            timestamp,
        })
    }
}

#[derive(Debug, Error)]
pub enum TransportInfoTxtError {
    #[error("invalid size of separated info parts, expected 2, given: {0}")]
    Size(usize),

    #[error(transparent)]
    Signature(#[from] IdentityError),

    #[error(transparent)]
    Timestamp(#[from] ParseIntError),
}
