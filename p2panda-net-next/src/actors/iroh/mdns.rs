// SPDX-License-Identifier: MIT OR Apache-2.0

use std::pin::Pin;

use futures_util::{Stream, StreamExt};
use iroh::discovery::UserData;
use iroh::discovery::mdns::{DiscoveryEvent, MdnsDiscovery};
use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef, call, registry};
use tracing::{trace, warn};

use crate::actors::address_book::{ADDRESS_BOOK, ToAddressBook};
use crate::actors::iroh::UserDataTransportInfo;
use crate::actors::{ActorNamespace, with_namespace};
use crate::config::MdnsDiscoveryMode;
use crate::utils::to_public_key;
use crate::{NodeId, NodeInfo, TransportInfo};

pub const MDNS_DISCOVERY: &str = "net.iroh.mdns";

pub enum ToMdns {
    Initialise(iroh::EndpointId, MdnsDiscoveryMode),

    NextStreamEvent,

    /// mDNS discovery service found an updated endpoint address.
    ///
    /// Since this came from an external discovery source we now need to translate this information
    /// into our "meta" transport info types.
    DiscoveredEndpointInfo {
        endpoint_id: iroh::PublicKey,
        endpoint_addr: Option<iroh::EndpointAddr>,
        // This is populated with our needed extra transport info since we called
        // `set_user_data_for_discovery` on the iroh endpoint.
        user_data: Option<UserData>,
    },
}

pub type MdnsArguments = (ActorNamespace, iroh::EndpointId, MdnsDiscoveryMode);

pub struct MdnsState {
    actor_namespace: ActorNamespace,
    stream: Option<Pin<Box<dyn Stream<Item = DiscoveryEvent>>>>,
}

impl MdnsState {
    pub async fn update_address_book(
        &self,
        node_id: NodeId,
        transport_info: TransportInfo,
    ) -> Result<(), ActorProcessingErr> {
        let address_book_ref = {
            let Some(actor) =
                registry::where_is(with_namespace(ADDRESS_BOOK, &self.actor_namespace))
            else {
                // Address book is not reachable, so we're probably shutting down.
                return Ok(());
            };
            ActorRef::<ToAddressBook>::from(actor)
        };

        // Update existing node info about us if available or create a new one.
        let mut node_info = match call!(address_book_ref, ToAddressBook::NodeInfo, node_id)? {
            Some(node_info) => node_info,
            None => NodeInfo::new(node_id),
        };
        node_info.update_transports(transport_info)?;
        let _ = call!(address_book_ref, ToAddressBook::InsertNodeInfo, node_info)?;

        Ok(())
    }
}

#[derive(Default)]
pub struct Mdns;

impl ThreadLocalActor for Mdns {
    type Msg = ToMdns;

    type State = MdnsState;

    type Arguments = MdnsArguments;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (actor_namespace, endpoint_id, mode) = args;

        // Automatically initialise mDNS service after starting actor.
        myself.send_message(ToMdns::Initialise(endpoint_id, mode))?;

        Ok(MdnsState {
            actor_namespace,
            stream: None,
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ToMdns::Initialise(endpoint_id, mode) => {
                if !mode.is_active() {
                    return Ok(());
                }

                let mdns = MdnsDiscovery::builder()
                    // Do not advertise our own endpoint address if in "passive" mode.
                    .advertise(mode.is_active())
                    .build(endpoint_id)?;

                // NOTE: We're _not_ hooking iroh's endpoint into this service (iroh would use the
                // resolving interface) as we're already handling that ourselves with checked and
                // authenticated addresses.

                // Start polling async stream of incoming discovery events.
                state.stream = Some(Box::pin(mdns.subscribe().await));
                myself.send_message(ToMdns::NextStreamEvent)?;
            }
            ToMdns::NextStreamEvent => {
                let Some(ref mut stream) = state.stream else {
                    unreachable!("tried to poll from mdns stream before initialising");
                };

                match stream.next().await {
                    Some(DiscoveryEvent::Discovered { endpoint_info, .. }) => {
                        myself.send_message(ToMdns::DiscoveredEndpointInfo {
                            endpoint_id: endpoint_info.endpoint_id,
                            user_data: endpoint_info.user_data().cloned(),
                            endpoint_addr: Some(endpoint_info.into()),
                        })?;
                    }
                    Some(DiscoveryEvent::Expired { .. }) => {
                        // At this point we know another node has not responded anymore within the
                        // local network.
                        //
                        // We can't do much here though since "removing" the iroh endpoint address
                        // from the transport info would need to be signed, and we don't have a
                        // signature here anymore.
                        //
                        // Additionally we don't know if that node might actually still be
                        // reachable (just not inside the same local area network).
                    }
                    None => {
                        // The stream has seized, close actor.
                        myself.stop(Some("mdns stream stopped".into()));
                    }
                }
            }
            ToMdns::DiscoveredEndpointInfo {
                endpoint_id,
                endpoint_addr,
                user_data,
            } => {
                let Some(user_data) = user_data else {
                    trace!(
                        %endpoint_id,
                        "ignore discovered endpoint addr, it doesn't contain any user data"
                    );
                    return Ok(());
                };

                match UserDataTransportInfo::try_from(user_data) {
                    Ok(txt) => {
                        // Assemble a transport info manually by combining the extra user data (
                        let transport_info = TransportInfo {
                            timestamp: txt.timestamp,
                            signature: txt.signature,
                            addresses: endpoint_addr
                                .map(|addr| vec![addr.into()])
                                .unwrap_or(vec![]),
                        };

                        // Check authenticity.
                        if transport_info.verify(&to_public_key(endpoint_id)).is_err() {
                            warn!(
                                %endpoint_id,
                                "found invalid transport info coming from iroh's services"
                            );
                            return Ok(());
                        }

                        state
                            .update_address_book(to_public_key(endpoint_id), transport_info)
                            .await?;
                    }
                    Err(err) => {
                        trace!(
                            %endpoint_id,
                            "ignore discovered endpoint addr from iroh's services, it contain's unparseable: {err:#?}"
                        );
                        return Ok(());
                    }
                }
            }
        }
        Ok(())
    }
}
