// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::BTreeSet;
use std::pin::Pin;

use futures_util::{FutureExt, Stream, StreamExt};
use iroh::discovery::{Discovery, DiscoveryError, DiscoveryItem, EndpointData, EndpointInfo};
use p2panda_discovery::address_book::NodeInfo;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tracing::{Instrument, error, info_span, trace, warn};

use crate::UnsignedTransportInfo;
use crate::actors::address_book::{update_address_book, watch_node_info};
use crate::actors::{ActorNamespace, generate_actor_namespace};
use crate::args::ApplicationArguments;
use crate::utils::{from_public_key, to_public_key};

/// Discovery service for iroh connecting iroh's endpoint with our address book actor. This
/// implements iroh's `Discovery` trait.
///
/// The endpoint can "resolve" node ids to iroh's endpoint addresses and inform the address book
/// about our own, changed address (for example if the home relay changed or we got an direct IP
/// address, etc., in iroh this is called "publish").
#[derive(Debug)]
pub struct AddressBookDiscovery {
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
                update_address_book(actor_namespace, public_key, transport_info.clone().into())
                    .await
            {
                warn!("could not update address book with own transport info: {err:#?}");
            }
        });
    }

    fn resolve(
        &self,
        endpoint_id: iroh::EndpointId,
    ) -> Option<BoxStream<Result<DiscoveryItem, DiscoveryError>>> {
        let actor_namespace = self.actor_namespace.clone();

        let span = info_span!("resolve", endpoint_id = %endpoint_id.fmt_short());
        trace!(parent: &span, "try to resolve endpoint id");

        let stream = async move {
            let subscription = watch_node_info(actor_namespace, to_public_key(endpoint_id), false)
                .await
                .map_err(|_| {
                    DiscoveryError::from_err_any(
                        PROVENANCE,
                        "address book actor did not respond with subscription",
                    )
                });

            match subscription {
                Ok(rx) => UnboundedReceiverStream::new(rx)
                    .filter_map(|event| async {
                        match event.value {
                            Some(node_info) => {
                                // Abort resolving if node info has been marked as "stale".
                                if node_info.is_stale() {
                                    return Some(Err(DiscoveryError::from_err_any(
                                        PROVENANCE,
                                        "node is marked as stale",
                                    )));
                                }

                                match iroh::EndpointAddr::try_from(node_info) {
                                    Ok(endpoint_addr) => {
                                        let info = EndpointInfo::from(endpoint_addr);
                                        Some(Ok(DiscoveryItem::new(info, PROVENANCE, None)))
                                    }
                                    Err(_) => {
                                        // No iroh-related transport information was available,
                                        // ignore this event and wait ..
                                        None
                                    }
                                }
                            }
                            None => {
                                // No node info was available in the address book yet, ignore this
                                // event and wait ..
                                None
                            }
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

type BoxStream<T> = Pin<Box<dyn Stream<Item = T> + Send + 'static>>;
