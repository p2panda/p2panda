// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::BTreeSet;
use std::pin::Pin;
use std::sync::Arc;

use futures_util::{FutureExt, Stream, StreamExt};
use iroh::discovery::{Discovery, DiscoveryError, DiscoveryItem, EndpointData, EndpointInfo};
use p2panda_core::PrivateKey;
use p2panda_discovery::address_book::NodeInfo as _;
use tokio::sync::Semaphore;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tracing::{Instrument, error, info_span, trace, warn};

use crate::address_book::AddressBook;
use crate::addrs::{NodeTransportInfo, UnsignedTransportInfo};
use crate::iroh_endpoint::{from_public_key, to_public_key};

/// Discovery service for iroh connecting iroh's endpoint with our address book actor. This
/// implements iroh's `Discovery` trait.
///
/// The endpoint can "resolve" node ids to iroh's endpoint addresses and inform the address book
/// about our own, changed address (for example if the home relay changed or we got an direct IP
/// address, etc., in iroh this is called "publish").
#[derive(Debug)]
pub struct AddressBookDiscovery {
    private_key: PrivateKey,
    address_book: AddressBook,
    semaphore: Arc<Semaphore>,
}

/// Identifies source of discovered item.
const PROVENANCE: &str = "address_book";

impl AddressBookDiscovery {
    pub fn new(private_key: PrivateKey, address_book: AddressBook) -> Self {
        Self {
            private_key,
            address_book,
            semaphore: Arc::new(Semaphore::new(1)),
        }
    }
}

impl Discovery for AddressBookDiscovery {
    fn publish(&self, data: &EndpointData) {
        let private_key = self.private_key.clone();
        let public_key = private_key.public_key();
        let data = data.to_owned();
        let semaphore = self.semaphore.clone();
        let address_book = self.address_book.clone();

        tokio::task::spawn(async move {
            // Get current transport info state and strictly serialize reading it to avoid race
            // conditions where multiple spawned "publish" tasks race against each other.
            let Ok(_permit) = semaphore.acquire().await else {
                error!("failed acquiring semaphore permit");
                return;
            };

            let Ok(node_info) = address_book.node_info(public_key).await else {
                error!("failed getting own transport info from address book");
                return;
            };
            let previous_transport_info = node_info.and_then(|info| info.transports());

            // Create transport info with iroh endpoint addresses if given. If no address exists
            // (because we are not reachable) we're explicitly making the address array empty to inform
            // other nodes about this.
            let Ok(transport_info) = if data.has_addrs() {
                UnsignedTransportInfo::from_addrs([iroh::EndpointAddr {
                    id: from_public_key(public_key),
                    addrs: BTreeSet::from_iter(data.addrs().cloned()),
                }
                .into()])
            } else {
                UnsignedTransportInfo::new()
            }
            .increment_timestamp(previous_transport_info.as_ref())
            .sign(&private_key) else {
                error!("failed signing own transport info");
                return;
            };

            // Ignore endpoint data from iroh if nothing has changed.
            if let Some(previous) = previous_transport_info
                && transport_info.addresses() == previous.addresses()
            {
                return;
            }

            // Update entry about ourselves in address book to allow this information to propagate
            // in other discovery mechanisms or side-channels outside of iroh.
            if let Err(err) = address_book
                .insert_transport_info(public_key, transport_info.clone().into())
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
        let span = info_span!("resolve", endpoint_id = %endpoint_id.fmt_short());
        trace!(parent: &span, "try to resolve endpoint id");

        let address_book = self.address_book.clone();

        let stream = async move {
            let subscription = address_book
                .watch_node_info(to_public_key(endpoint_id), false)
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
