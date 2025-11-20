// SPDX-License-Identifier: MIT OR Apache-2.0

mod connection;
mod endpoint;
#[cfg(feature = "mdns")]
mod mdns;
#[cfg(test)]
mod tests;

use std::collections::BTreeSet;
use std::num::ParseIntError;
use std::pin::Pin;
use std::str::FromStr;

use futures_util::{FutureExt, Stream, StreamExt};
use iroh::discovery::{
    Discovery, DiscoveryError, DiscoveryItem, EndpointData, EndpointInfo, UserData,
};
use iroh::endpoint_info::MaxLengthExceededError;
use iroh::protocol::ProtocolHandler;
use p2panda_core::{IdentityError, Signature};
use ractor::{ActorRef, call, registry};
use thiserror::Error;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tracing::{Instrument, debug, error, info_span, trace, warn};

use crate::actors::address_book::{ADDRESS_BOOK, ImmediateResult, NodeEvent, ToAddressBook};
use crate::actors::iroh::connection::ConnectionActorError;
use crate::actors::{ActorNamespace, generate_actor_namespace, with_namespace};
use crate::addrs::NodeId;
use crate::args::ApplicationArguments;
use crate::utils::{from_public_key, to_public_key};
use crate::{NodeInfoError, TransportInfo, UnsignedTransportInfo};

pub use endpoint::{IROH_ENDPOINT, IrohEndpoint, ToIrohEndpoint};
#[cfg(feature = "mdns")]
pub use mdns::{MDNS_DISCOVERY, Mdns, ToMdns};

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

        let mut parts = parts.iter();
        let signature_str = parts.next().expect("we've checked the size before");
        let timestamp_str = parts.next().expect("we've checked the size before");

        // Try to parse halfs into signature and timestamp.
        let signature = Signature::from_str(signature_str)?;
        let timestamp = u64::from_str(timestamp_str)?;

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

/// Discovery service for iroh connecting iroh's endpoint with our address book actor. This
/// implements iroh's `Discovery` trait.
///
/// The endpoint can "resolve" node ids to iroh's endpoint addresses and inform the address book
/// about our own, changed address (for example if the home relay changed or we got an direct IP
/// address, etc., in iroh this is called "publish").
//
// NOTE: Strictly speaking we only need the "resolver" part for iroh-gossip right now, as our own
// protocols call the `connect` helper method which takes an endpoint address from the address book
// and returns an error if there's no information given (so we _never_ call an iroh endpoint with
// only the endpoint id).
#[derive(Debug)]
struct AddressBookDiscovery {
    actor_namespace: ActorNamespace,
    args: ApplicationArguments,
}

/// Identifies source of discovered item.
const PROVENANCE: &str = "address book";

impl AddressBookDiscovery {
    pub fn new(args: ApplicationArguments) -> Self {
        Self {
            actor_namespace: generate_actor_namespace(&args.public_key),
            args,
        }
    }
}

impl Discovery for AddressBookDiscovery {
    fn publish(&self, data: &EndpointData) {
        // Create a new transport info with iroh addresses if given. If no iroh address exists
        // (because we are not reachable) we're explicitly making the address array empty to inform
        // other nodes about this.
        let Ok(transport_info) = if data.has_addrs() {
            UnsignedTransportInfo::from_addrs([iroh::EndpointAddr {
                id: from_public_key(self.args.public_key),
                addrs: BTreeSet::from_iter(data.addrs().cloned()),
            }
            .into()])
        } else {
            UnsignedTransportInfo::new()
        }
        .sign(&self.args.private_key) else {
            error!("failed signing own transport info");
            return;
        };

        let actor_namespace = self.actor_namespace.clone();
        let public_key = self.args.public_key;

        tokio::task::spawn(async move {
            // Update entry about ourselves in address book to allow this information to propagate
            // in other discovery mechanisms or side-channels outside of iroh.
            if let Err(err) =
                update_address_book(actor_namespace, public_key, transport_info.clone()).await
            {
                warn!("could not update address book with own transport info: {err:#?}");
            } else {
                debug!(%transport_info, "updated our iroh endpoint address");
            }
        });
    }

    fn resolve(
        &self,
        endpoint_id: iroh::EndpointId,
    ) -> Option<BoxStream<Result<DiscoveryItem, DiscoveryError>>> {
        let actor_namespace = self.actor_namespace.clone();

        let span = info_span!("resolve", endpoint_id = %endpoint_id.fmt_short());
        trace!(parent: &span, "received request to resolve endpoint id");

        let stream = async move {
            let subscription =
                subscribe_to_node_info(actor_namespace, to_public_key(endpoint_id), true)
                    .await
                    .ok_or(DiscoveryError::from_err_any(
                        PROVENANCE,
                        "address book actor did not respond with subscription",
                    ));

            match subscription {
                Ok(rx) => BroadcastStream::new(rx)
                    .map(|event| match event {
                        Ok(event) => match iroh::EndpointAddr::try_from(event.node_info) {
                            Ok(endpoint_addr) => {
                                let info = EndpointInfo::from(endpoint_addr);
                                Ok(DiscoveryItem::new(info, PROVENANCE, None))
                            }
                            Err(err) => {
                                warn!("failed resolving address: {err:#?}");
                                Err(DiscoveryError::from_err(PROVENANCE, err))
                            }
                        },
                        Err(err) => {
                            warn!("failed resolving address: {err:#?}");
                            Err(DiscoveryError::from_err(PROVENANCE, err))
                        }
                    })
                    .boxed(),
                Err(err) => {
                    warn!("failed resolving address due to actor error: {err:#?}");
                    futures_util::stream::once(async { Err(err) }).boxed()
                }
            }
        }
        .instrument(span);

        Some(Box::pin(stream.flatten_stream()))
    }
}

// @TODO: We can probably factor all of these "address book helper" methods out into an own
// "utils-like" mod so it can be used by other actors as well (where he have similar code already).

async fn address_book_ref(actor_namespace: ActorNamespace) -> Option<ActorRef<ToAddressBook>> {
    registry::where_is(with_namespace(ADDRESS_BOOK, &actor_namespace))
        .map(ActorRef::<ToAddressBook>::from)
}

async fn update_address_book(
    actor_namespace: ActorNamespace,
    node_id: NodeId,
    transport_info: TransportInfo,
) -> Result<(), AddressBookDiscoveryError> {
    let Some(address_book_ref) = address_book_ref(actor_namespace).await else {
        return Err(AddressBookDiscoveryError::ActorNotAvailable);
    };

    let _ = call!(
        address_book_ref,
        ToAddressBook::InsertTransportInfo,
        node_id,
        transport_info
    )
    .map_err(|_| AddressBookDiscoveryError::ActorFailed)?;

    Ok(())
}

async fn subscribe_to_node_info(
    actor_namespace: ActorNamespace,
    node_id: NodeId,
    immediate: ImmediateResult,
) -> Option<broadcast::Receiver<NodeEvent>> {
    let Some(address_book_ref) = address_book_ref(actor_namespace).await else {
        // Address book is not reachable, so we're probably shutting down.
        return None;
    };

    let Ok(rx) = call!(
        address_book_ref,
        ToAddressBook::SubscribeNodeChanges,
        node_id,
        immediate
    ) else {
        return None;
    };

    Some(rx)
}

#[derive(Debug, Error)]
pub enum AddressBookDiscoveryError {
    #[error("address book actor is not available")]
    ActorNotAvailable,

    #[error("address book actor failed")]
    ActorFailed,

    #[error("could not update transport information: {0}")]
    UpdateFailed(#[from] NodeInfoError),
}

type BoxStream<T> = Pin<Box<dyn Stream<Item = T> + Send + 'static>>;
