// SPDX-License-Identifier: MIT OR Apache-2.0

use std::pin::Pin;

use futures_util::{Stream, StreamExt};
use iroh::discovery::mdns::{DiscoveryEvent, MdnsDiscovery};
use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef};

use crate::actors::iroh::ToIrohEndpoint;
use crate::config::MdnsDiscoveryMode;

pub const MDNS_DISCOVERY: &str = "net.iroh.mdns";

pub enum ToMdns {
    Initialise(iroh::EndpointId, MdnsDiscoveryMode),
    NextStreamEvent,
}

pub type MdnsArguments = (
    iroh::EndpointId,
    MdnsDiscoveryMode,
    ActorRef<ToIrohEndpoint>,
);

pub struct MdnsState {
    iroh_endpoint_ref: ActorRef<ToIrohEndpoint>,
    stream: Option<Pin<Box<dyn Stream<Item = DiscoveryEvent>>>>,
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
        let (endpoint_id, mode, iroh_endpoint_ref) = args;

        // Automatically initialise mDNS service after starting actor.
        myself.send_message(ToMdns::Initialise(endpoint_id, mode))?;

        Ok(MdnsState {
            iroh_endpoint_ref,
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
                        let _ = state.iroh_endpoint_ref.send_message(
                            ToIrohEndpoint::UpdatedEndpointAddr {
                                endpoint_id: endpoint_info.endpoint_id,
                                user_data: endpoint_info.user_data().cloned(),
                                endpoint_addr: Some(endpoint_info.into()),
                            },
                        );
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
        }
        Ok(())
    }
}
