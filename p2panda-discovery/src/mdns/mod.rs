// SPDX-License-Identifier: MIT OR Apache-2.0

//! Local peer discovery via mDNS over IPv4.
mod dns;
mod socket;

use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;

use anyhow::Result;
use flume::Sender;
use futures_lite::{FutureExt, StreamExt};
use hickory_proto::rr::Name;
use iroh::NodeAddr;
use netwatch::netmon::Monitor;
use tokio::sync::mpsc::{self, Receiver};
use tokio_util::task::AbortOnDropHandle;
use tracing::{debug, warn};

use crate::mdns::dns::{MulticastDNSMessage, make_query, make_response, parse_message};
use crate::mdns::socket::{send, socket_v4, socket_v4_unbound};
use crate::{BoxedStream, Discovery, DiscoveryEvent};

const MDNS_PROVENANCE: &str = "mdns";
const MDNS_QUERY_INTERVAL: Duration = Duration::from_millis(1000);
const SOCKET_REBIND_INTERVAL: Duration = Duration::from_millis(5000);

pub type ServiceName = Name;

type SubscribeSender = Sender<Result<DiscoveryEvent>>;

enum Message {
    Subscribe(ServiceName, SubscribeSender),
    UpdateLocalAddress(NodeAddr),
}

#[derive(Debug)]
pub struct LocalDiscovery {
    #[allow(dead_code)]
    handle: AbortOnDropHandle<()>,
    tx: Sender<Message>,
}

/// Create a new network monitor and subscribe to major interface changes.
async fn network_monitor() -> Result<Receiver<bool>> {
    let network_monitor = Monitor::new().await?;
    let (interface_change_tx, interface_change_rx) = mpsc::channel(8);
    let _token = network_monitor
        .subscribe(move |is_major| {
            debug!("detected major network interface change");
            let interface_change_tx = interface_change_tx.clone();
            async move {
                interface_change_tx.send(is_major).await.ok();
            }
            .boxed()
        })
        .await?;

    Ok(interface_change_rx)
}

impl Default for LocalDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalDiscovery {
    pub fn new() -> Self {
        let (tx, rx) = flume::bounded(64);

        let mut socket_is_bound = false;
        let mut socket = match socket_v4() {
            Ok(socket) => {
                socket_is_bound = true;
                socket
            }
            Err(err) => {
                warn!("failed to create udp socket for mdns discovery: {}", err);
                socket_v4_unbound().expect("create udp socket")
            }
        };

        let mut subscribers: HashMap<ServiceName, Vec<SubscribeSender>> = HashMap::new();
        let mut my_node_addr: Option<NodeAddr> = None;

        let handle = tokio::task::spawn(async move {
            let mut interface_change_rx = network_monitor().await.expect("start network monitor");
            let mut socket_interval = tokio::time::interval(SOCKET_REBIND_INTERVAL);
            let mut interval = tokio::time::interval(MDNS_QUERY_INTERVAL);
            let mut buf = [0; 1472];

            loop {
                tokio::select! {
                    biased;
                    Some(true) = interface_change_rx.recv() => {
                        // Force a recreation of the socket on the next tick.
                        socket_is_bound = false;
                    }
                    Ok(len) = socket.recv(&mut buf), if socket_is_bound => {
                        let Some(msg) = parse_message(&buf[..len]) else {
                            continue;
                        };

                        match msg {
                            MulticastDNSMessage::Query(service_name) => {
                                let Some(my_node_addr) = &my_node_addr else {
                                    continue;
                                };

                                if subscribers.contains_key(&service_name) {
                                    let response = make_response(&service_name, my_node_addr);
                                    send(&socket, response).await;
                                }
                            },
                            MulticastDNSMessage::Response(service_name, node_addrs) => {
                                let Some(my_node_addr) = &my_node_addr else {
                                    continue;
                                };

                                let Some(subscribers) = subscribers.get(&service_name) else {
                                    continue;
                                };

                                for subscribe_tx in subscribers {
                                    for node_addr in &node_addrs {
                                        if node_addr.node_id == my_node_addr.node_id {
                                            continue;
                                        }

                                        subscribe_tx
                                            .send_async(Ok(DiscoveryEvent {
                                                provenance: MDNS_PROVENANCE,
                                                node_addr: node_addr.clone(),
                                            }))
                                            .await
                                            .ok();
                                    }
                                }
                            }
                        }
                    },
                    _ = interval.tick(), if socket_is_bound => {
                        for service_name in subscribers.keys() {
                            send(&socket, make_query(service_name)).await;
                        }
                    },
                    Ok(msg) = rx.recv_async(), if socket_is_bound => {
                        match msg {
                            Message::Subscribe(service_name, subscribe_tx) => {
                                if let Some(subscriber) = subscribers.get_mut(&service_name) {
                                    subscriber.push(subscribe_tx);
                                } else {
                                    subscribers.insert(service_name, vec![subscribe_tx]);
                                }
                            }
                            Message::UpdateLocalAddress(ref addr) => {
                                my_node_addr = Some(addr.clone());
                            }
                        }
                    },
                    _ = socket_interval.tick() => {
                        if !socket_is_bound {
                            match socket_v4() {
                                Ok(bound_socket) => {
                                    socket = bound_socket;
                                    debug!("bound udp socket for mdns discovery");
                                    socket_is_bound = true;
                                }
                                Err(err) => warn!("failed to rebind socket: {}", err)
                            }
                        }
                    }
                    else => break,
                }
            }
        });

        Self {
            handle: AbortOnDropHandle::new(handle),
            tx,
        }
    }
}

impl Discovery for LocalDiscovery {
    fn subscribe(&self, network_id: [u8; 32]) -> Option<BoxedStream<Result<DiscoveryEvent>>> {
        let (subscribe_tx, subscribe_rx) = flume::bounded(16);
        let service_tx = self.tx.clone();
        let name = format!(
            "_{}._udp.local.",
            base32::encode(base32::Alphabet::Z, &network_id)
        );
        let service_name = Name::from_str(&name).expect("correctly formatted DNS name");

        tokio::spawn(async move {
            service_tx
                .send_async(Message::Subscribe(service_name, subscribe_tx))
                .await
                .ok();
        });

        Some(subscribe_rx.into_stream().boxed())
    }

    fn update_local_address(&self, addr: &NodeAddr) -> Result<()> {
        let tx = self.tx.clone();
        let addr = addr.clone();
        tokio::spawn(async move {
            tx.send_async(Message::UpdateLocalAddress(addr)).await.ok();
        });
        Ok(())
    }
}
