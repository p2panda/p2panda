// SPDX-License-Identifier: MIT OR Apache-2.0

use futures_util::StreamExt;
use iroh::discovery::mdns::{DiscoveryEvent, MdnsDiscovery};
use iroh::discovery::{Discovery, EndpointData, UserData};
use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef};
use tokio::task::JoinHandle;
use tracing::{debug, trace, warn};

use crate::actors::address_book::{update_address_book, watch_node_info};
use crate::actors::{ActorNamespace, generate_actor_namespace};
use crate::addrs::{AuthenticatedTransportInfo, NodeInfo, NodeTransportInfo, TransportInfo};
use crate::config::MdnsDiscoveryMode;
use crate::iroh::user_data::UserDataTransportInfo;
use crate::test_utils::ApplicationArguments;
use crate::utils::{from_public_key, to_public_key};

pub const MDNS_DISCOVERY: &str = "net.iroh.mdns";

const MDNS_SERVICE_NAME: &str = "p2pandav1";

#[allow(clippy::large_enum_variant)]
pub enum ToMdns {
    /// Start mDNS "ambient" discovery.
    Initialise(iroh::EndpointId, MdnsDiscoveryMode),

    /// Address book informed us about our own node info being updated.
    UpdateNodeInfo(NodeInfo),

    /// mDNS discovery service found an updated endpoint address.
    ///
    /// Since this came from an external discovery source we now need to translate this information
    /// into our "meta" transport info types.
    DiscoveredEndpointInfo {
        endpoint_id: iroh::PublicKey,
        endpoint_addr: Option<iroh::EndpointAddr>,
        user_data: Option<UserData>,
    },
}

pub struct MdnsState {
    actor_namespace: ActorNamespace,
    args: ApplicationArguments,
    service: Option<MdnsDiscovery>,
    handle: Option<JoinHandle<()>>,
}

#[derive(Default)]
pub struct Mdns;

impl ThreadLocalActor for Mdns {
    type Msg = ToMdns;

    type State = MdnsState;

    type Arguments = ApplicationArguments;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let actor_namespace = generate_actor_namespace(&args.public_key);

        // Automatically initialise mDNS service after starting actor.
        myself.send_message(ToMdns::Initialise(
            from_public_key(args.public_key),
            args.iroh_config.mdns_discovery_mode.clone(),
        ))?;

        Ok(MdnsState {
            actor_namespace,
            args,
            service: None,
            handle: None,
        })
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        if let Some(handle) = &state.handle {
            handle.abort();
        }

        Ok(())
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ToMdns::Initialise(endpoint_id, mode) => {
                debug!("initialise mdns discovery service in {mode} mode");

                if !mode.is_active() {
                    return Ok(());
                }

                // NOTE: We're _not_ hooking iroh's endpoint into this service (iroh would use the
                // resolving interface) as we're already handling that ourselves with checked and
                // authenticated addresses.

                let mdns = MdnsDiscovery::builder()
                    // Do not advertise our own endpoint address if in "passive" mode.
                    .advertise(mode.is_active())
                    .service_name(MDNS_SERVICE_NAME)
                    .build(endpoint_id)?;

                let handle = {
                    let myself = myself.clone();

                    // Subscribe to incoming discovery events of mDNS service.
                    let mut stream = mdns.subscribe().await;

                    // Subscribe to address book to listen to changes of our own node info.
                    let mut rx = watch_node_info(
                        state.actor_namespace.clone(),
                        state.args.public_key,
                        // Disable "updates only" to inform mdns about our current transport info
                        // as soon as possible.
                        false,
                    )
                    .await?;

                    tokio::task::spawn(async move {
                        loop {
                            tokio::select! {
                                event = stream.next() => {
                                    match event {
                                        Some(DiscoveryEvent::Discovered { endpoint_info, .. }) => {
                                            let _ = myself.send_message(ToMdns::DiscoveredEndpointInfo {
                                                endpoint_id: endpoint_info.endpoint_id,
                                                user_data: endpoint_info.user_data().cloned(),
                                                endpoint_addr: Some(endpoint_info.into()),
                                            });
                                        }
                                        Some(DiscoveryEvent::Expired { .. }) => {
                                            // At this point we know another node has not responded anymore
                                            // within the local network.
                                            //
                                            // We can't do much here though since "removing" the iroh
                                            // endpoint address from the transport info would need to be
                                            // signed, and we don't have a signature here anymore.
                                            //
                                            // Additionally we don't know if that node might actually still
                                            // be reachable (just not inside the same local area network).
                                        }
                                        None => {
                                            // The stream has seized, close actor.
                                            myself.stop(Some("mdns stream stopped".into()));
                                        }
                                    }
                                },
                                Some(event) = rx.recv() => {
                                    if let Some(node_info) = event.value {
                                        let _ = myself.send_message(ToMdns::UpdateNodeInfo(node_info));
                                    }
                                }
                            }
                        }
                    })
                };

                state.handle = Some(handle);
                state.service = Some(mdns);
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
                        // Assemble a transport info manually by combining the extra user data
                        // (timestamp, signature) with actual addressing information from iroh.
                        let transport_info = AuthenticatedTransportInfo {
                            timestamp: txt.timestamp,
                            signature: txt.signature,
                            addresses: {
                                endpoint_addr
                                    .clone()
                                    .map(|mut addr| {
                                        // Optionally add relay url if it was delivered via user
                                        // data as well.
                                        if let Some(relay_url) = txt.relay_url {
                                            addr = addr.with_relay_url(relay_url);
                                        }
                                        vec![addr.into()]
                                    })
                                    .unwrap_or(vec![])
                            },
                        };

                        // Check authenticity.
                        if transport_info.verify(&to_public_key(endpoint_id)).is_err() {
                            warn!(
                                %endpoint_id,
                                "found invalid transport info coming from iroh's services"
                            );
                            return Ok(());
                        }

                        trace!(%endpoint_id, "discovered new transport info");

                        if let Err(err) = update_address_book(
                            state.actor_namespace.clone(),
                            to_public_key(endpoint_id),
                            transport_info.into(),
                        )
                        .await
                        {
                            warn!(
                                %endpoint_id,
                                "updating address book failed with error: {err:#?}"
                            );
                        }
                    }
                    Err(err) => {
                        trace!(
                            %endpoint_id,
                            "ignore discovered endpoint addr from mdns service, it contains unparseable user data: {err:#?}"
                        );
                        return Ok(());
                    }
                }
            }
            ToMdns::UpdateNodeInfo(node_info) => {
                trace!("received updated node info");
                let Ok(endpoint_addr) = TryInto::<iroh::EndpointAddr>::try_into(node_info.clone())
                else {
                    // Node info doesn't contain any iroh-related address information. This is
                    // unlikely to happen currently as our only transport is iroh.
                    return Ok(());
                };

                let transport_info = node_info
                    .transports
                    .expect("if there's an endpoint address then there's transport info");

                let TransportInfo::Authenticated(transport_info) = transport_info else {
                    // Only publish authenticated transport info.
                    return Ok(());
                };

                // Inform mDNS service about our updated transport info.
                if let Ok(user_data) = UserData::try_from(transport_info) {
                    state
                        .service
                        .as_ref()
                        .expect("exists at this point")
                        .publish(
                            &EndpointData::from(endpoint_addr).with_user_data(Some(user_data)),
                        );
                }
            }
        }
        Ok(())
    }
}
