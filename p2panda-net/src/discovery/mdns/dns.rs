// SPDX-License-Identifier: AGPL-3.0-or-later

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;

use hickory_proto::op::{Message, MessageType, Query};
use hickory_proto::rr::{rdata, DNSClass, Name, RData, Record, RecordType};
use iroh_net::{NodeAddr, NodeId};
use tracing::{debug, trace};

use crate::discovery::mdns::ServiceName;

pub enum MulticastDNSMessage {
    Query(ServiceName),
    Response(ServiceName, Vec<NodeAddr>),
}

pub fn make_query(service_name: &ServiceName) -> Message {
    let mut msg = Message::new();
    msg.set_message_type(MessageType::Query);
    let mut query = Query::new();
    query.set_query_class(DNSClass::IN);
    query.set_query_type(RecordType::PTR);
    query.set_name(service_name.clone());
    msg.add_query(query);
    msg
}

pub fn make_response(service_name: &ServiceName, node_addr: &NodeAddr) -> Message {
    let mut msg = Message::new();
    msg.set_message_type(MessageType::Response);
    msg.set_authoritative(true);

    let node_id_str = node_addr.node_id.to_string();

    let my_srv_name = Name::from_str(&node_id_str)
        .expect("node id was checked already")
        .append_domain(service_name)
        .expect("was checked already");

    let mut srv_map = BTreeMap::new();
    for addr in node_addr.direct_addresses() {
        srv_map
            .entry(addr.port())
            .or_insert_with(Vec::new)
            .push(addr.ip());
    }

    for (port, addrs) in srv_map {
        let target = Name::from_str(&format!("{}-{}.local.", node_id_str, port))
            .expect("node was checked already");
        msg.add_answer(Record::from_rdata(
            my_srv_name.clone(),
            0,
            RData::SRV(rdata::SRV::new(0, 0, port, target.clone())),
        ));
        for addr in addrs {
            match addr {
                IpAddr::V4(addr) => {
                    msg.add_additional(Record::from_rdata(
                        target.clone(),
                        0,
                        RData::A(rdata::A::from(addr)),
                    ));
                }
                IpAddr::V6(addr) => {
                    msg.add_additional(Record::from_rdata(
                        target.clone(),
                        0,
                        RData::AAAA(rdata::AAAA::from(addr)),
                    ));
                }
            }
        }
    }

    msg
}

pub fn parse_message(bytes: &[u8]) -> Option<MulticastDNSMessage> {
    let message = match Message::from_vec(bytes) {
        Ok(packet) => packet,
        Err(err) => {
            debug!("error parsing mdns packet: {}", err);
            return None;
        }
    };

    if let Some(query) = parse_query(&message) {
        return Some(query);
    }

    if let Some(response) = parse_response(&message) {
        return Some(response);
    }

    None
}

fn parse_query(message: &Message) -> Option<MulticastDNSMessage> {
    for query in message.queries() {
        if query.query_class() != DNSClass::IN {
            trace!(
                "received mdns query with wrong class {}",
                query.query_class()
            );
            continue;
        }
        if query.query_type() != RecordType::PTR {
            trace!("received mDNS query with wrong type {}", query.query_type());
            continue;
        }

        let service_name = query.name();

        trace!("received mDNS query for {}", query.name());
        return Some(MulticastDNSMessage::Query(service_name.clone()));
    }

    None
}

fn parse_response(message: &Message) -> Option<MulticastDNSMessage> {
    let mut peer_ports: BTreeMap<Name, Vec<(u16, NodeId)>> = BTreeMap::new();
    let mut service_name: Option<ServiceName> = None;

    for answer in message.answers() {
        if answer.dns_class() != DNSClass::IN {
            trace!(
                "received mdns response with wrong class {:?}",
                answer.dns_class()
            );
            continue;
        }
        let name = answer.name();
        service_name = match service_name {
            Some(name) => {
                if name != name.base_name() {
                    trace!("received mdns response with wrong service {}", name);
                }
                Some(name)
            }
            None => Some(name.base_name()),
        };
        debug!("received mdns response for {}", name);
        let node_id = {
            let Some(node_id_bytes) = name.iter().next() else {
                continue;
            };
            let Cow::Borrowed(node_id_str) = String::from_utf8_lossy(node_id_bytes) else {
                debug!(
                    "received mdns response with invalid node id {:?}",
                    node_id_bytes
                );
                continue;
            };
            let Ok(node_id) = NodeId::from_str(node_id_str) else {
                debug!(
                    "received mdns response with invalid node id {:?}",
                    node_id_bytes
                );
                continue;
            };
            node_id
        };
        let Some(RData::SRV(srv)) = answer.data() else {
            trace!("received mdns response with wrong data {:?}", answer.data());
            continue;
        };
        peer_ports
            .entry(srv.target().clone())
            .or_default()
            .push((srv.port(), node_id));
    }

    let local = Name::from_str("local.").unwrap();
    let mut peer_addrs: BTreeMap<NodeId, Vec<(IpAddr, u16)>> = BTreeMap::new();
    for additional in message.additionals() {
        if additional.dns_class() != DNSClass::IN {
            trace!(
                "received mdns additional with wrong class {:?}",
                additional.dns_class()
            );
            continue;
        }
        let name = additional.name();
        if name.base_name() != local {
            trace!("received mdns additional for wrong service {}", name);
            continue;
        }
        trace!("received mdns additional for {}", name);
        let ip: IpAddr = match additional.data() {
            Some(RData::A(addr)) => addr.0.into(),
            Some(RData::AAAA(addr)) => addr.0.into(),
            _ => {
                debug!(
                    "received mdns additional with wrong data {:?}",
                    additional.data()
                );
                continue;
            }
        };
        for (port, peer_id) in peer_ports.get(name).map(|x| &**x).unwrap_or(&[]) {
            peer_addrs.entry(*peer_id).or_default().push((ip, *port));
        }
    }

    if peer_addrs.is_empty() {
        return None;
    }

    let mut deduped = BTreeMap::new();
    for (peer_id, mut addrs) in peer_addrs {
        addrs.sort_unstable();
        addrs.dedup();
        deduped.insert(peer_id, addrs);
    }

    let mut ret = Vec::new();
    for (peer_id, addrs) in deduped.into_iter() {
        let direct_addresses: BTreeSet<SocketAddr> = addrs
            .iter()
            .map(|(ip, port)| SocketAddr::new(*ip, *port))
            .collect();

        ret.push(NodeAddr::new(peer_id).with_direct_addresses(direct_addresses));
    }

    match service_name {
        Some(service_name) => Some(MulticastDNSMessage::Response(service_name.clone(), ret)),
        None => {
            debug!("received mdns response without service name");
            None
        }
    }
}
