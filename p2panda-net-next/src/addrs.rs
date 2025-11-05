// SPDX-License-Identifier: MIT OR Apache-2.0

use std::hash::Hash as StdHash;
use std::mem;
use std::net::SocketAddr;

use iroh::{EndpointAddr, RelayUrl, TransportAddr};
use p2panda_core::cbor::encode_cbor;
use p2panda_core::{PrivateKey, Signature};
use p2panda_discovery::address_book;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{current_timestamp, from_public_key, to_public_key};

pub type NodeId = p2panda_core::PublicKey;

#[derive(Clone, Debug)]
pub struct NodeInfo {
    /// Unique identifier (Ed25519 public key) of this node.
    pub node_id: NodeId,

    /// Use node as a "bootstrap".
    ///
    /// Bootstraps are prioritized during discovery as they are considered "more reliable" and
    /// faster to reach than other nodes. Usually they are behind a static IP address and are
    /// always online.
    ///
    /// This is a local configuration and is not exchanged during discovery. Every node can decide
    /// themselves which other node they consider a bootstrap or not.
    //
    // @TODO(adz): Ideally we want to rely on connection metrics / behaviour etc. to "rate" a node
    // and this should become another factor to prioritize some nodes over others (at least for
    // initial discovery).
    pub bootstrap: bool,

    /// Transport protocols we can use to connect to this node.
    ///
    /// If `None` then no information was received and we can't connect yet.
    pub transports: Option<TransportInfo>,
}

impl NodeInfo {
    pub fn new(node_id: NodeId) -> Self {
        Self {
            node_id,
            bootstrap: false,
            transports: None,
        }
    }

    pub fn update_transports(&mut self, other: TransportInfo) -> Result<(), NodeInfoError> {
        // Make sure the given info matches the node id.
        for address in &other.addresses {
            #[allow(irrefutable_let_patterns)]
            if let TransportAddress::Iroh(endpoint_addr) = address
                && to_public_key(endpoint_addr.id) != self.node_id
            {
                return Err(NodeInfoError::NodeIdMismatch);
            }
        }

        // Choose "latest" info by checking timestamp if given.
        match self.transports.as_ref() {
            None => self.transports = Some(other),
            Some(current) => {
                if other.timestamp > current.timestamp {
                    self.transports = Some(other);
                }
            }
        }

        Ok(())
    }

    pub fn verify(&self) -> Result<(), NodeInfoError> {
        match self.transports {
            Some(ref transports) => transports.verify(&self.node_id),
            None => Ok(()),
        }
    }
}

impl TryFrom<NodeInfo> for EndpointAddr {
    type Error = NodeInfoError;

    fn try_from(node_info: NodeInfo) -> Result<Self, Self::Error> {
        let Some(transports) = node_info.transports else {
            return Err(NodeInfoError::MissingTransportAddresses);
        };

        transports
            .addresses
            .into_iter()
            .find_map(|address| match address {
                TransportAddress::Iroh(endpoint_addr) => Some(endpoint_addr),
                #[allow(unreachable_patterns)]
                _ => None,
            })
            .ok_or(NodeInfoError::MissingTransportAddresses)
    }
}

impl address_book::NodeInfo<NodeId> for NodeInfo {
    type Transports = TransportInfo;

    fn id(&self) -> NodeId {
        self.node_id
    }

    fn is_bootstrap(&self) -> bool {
        self.bootstrap
    }

    fn transports(&self) -> Option<Self::Transports> {
        self.transports.clone()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UnsignedTransportInfo {
    /// UNIX timestamp from when this transport information was published.
    ///
    /// This can be used to find out which information is the "latest".
    pub timestamp: u64,

    /// Associated transport addresses to aid establishing a connection to this node.
    pub addresses: Vec<TransportAddress>,
}

impl UnsignedTransportInfo {
    pub fn new() -> Self {
        Self {
            timestamp: current_timestamp(),
            addresses: vec![],
        }
    }

    pub fn from_addrs(addrs: impl IntoIterator<Item = TransportAddress>) -> Self {
        let mut info = Self::new();
        for addr in addrs {
            info.add_addr(addr);
        }
        info
    }

    /// Add transport address for this node.
    ///
    /// This method automatically de-duplicates transports per type and chooses the last-inserted
    /// one.
    pub fn add_addr(&mut self, addr: TransportAddress) {
        let existing_transport_index =
            self.addresses
                .iter()
                .enumerate()
                .find_map(|(index, existing_addr)| {
                    if mem::discriminant(&addr) == mem::discriminant(existing_addr) {
                        Some(index)
                    } else {
                        None
                    }
                });

        if let Some(index) = existing_transport_index {
            self.addresses.remove(index);
        }

        self.addresses.push(addr);
    }

    fn to_bytes(&self) -> Result<Vec<u8>, NodeInfoError> {
        let bytes = encode_cbor(&self)?;
        Ok(bytes)
    }

    /// Returns number of associated transports for this node.
    pub fn len(&self) -> usize {
        self.addresses.len()
    }

    pub fn is_empty(&self) -> bool {
        self.addresses.is_empty()
    }

    /// Authenticate transport info by signining it with our secret key.
    pub fn sign(self, signing_key: &PrivateKey) -> Result<TransportInfo, NodeInfoError> {
        Ok(TransportInfo {
            timestamp: self.timestamp,
            signature: {
                let bytes = self.to_bytes()?;
                signing_key.sign(&bytes)
            },
            addresses: self.addresses,
        })
    }
}

impl Default for UnsignedTransportInfo {
    fn default() -> Self {
        Self::new()
    }
}

impl From<EndpointAddr> for UnsignedTransportInfo {
    fn from(addr: EndpointAddr) -> Self {
        Self::from_addrs([addr.into()])
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransportInfo {
    /// UNIX timestamp from when this transport information was published.
    ///
    /// This can be used to find out which information is the "latest".
    pub timestamp: u64,

    /// Signature to prove authenticity of this transport information.
    ///
    /// Other nodes can validate the authenticity by checking this signature against the associated
    /// node id and info.
    ///
    /// This protects against attacks where nodes maliciously publish wrong information about other
    /// nodes, for example to make them unreachable due to invalid addresses.
    pub signature: Signature,

    /// Associated transport addresses to aid establishing a connection to this node.
    pub addresses: Vec<TransportAddress>,
}

impl TransportInfo {
    pub fn new_unsigned() -> UnsignedTransportInfo {
        UnsignedTransportInfo::new()
    }

    fn to_unsigned(&self) -> UnsignedTransportInfo {
        UnsignedTransportInfo {
            timestamp: self.timestamp,
            addresses: self.addresses.clone(),
        }
    }

    /// Returns number of associated transports for this node.
    pub fn len(&self) -> usize {
        self.addresses.len()
    }

    pub fn is_empty(&self) -> bool {
        self.addresses.is_empty()
    }

    pub fn verify(&self, node_id: &NodeId) -> Result<(), NodeInfoError> {
        let bytes = self.to_unsigned().to_bytes()?;
        if !node_id.verify(&bytes, &self.signature) {
            Err(NodeInfoError::InvalidSignature)
        } else {
            Ok(())
        }
    }
}

/// Associated transport addresses to aid establishing a connection to this node.
///
/// Currently this only supports using iroh (Internet Protocol) to connect.
#[derive(Clone, Debug, PartialEq, Eq, StdHash, Serialize, Deserialize)]
pub enum TransportAddress {
    /// Information to connect to another node via QUIC / UDP / IP using iroh for holepunching and
    /// relayed connections as a fallback.
    ///
    /// To connect to another node either their "home relay" URL needs to be known (to coordinate
    /// holepunching or relayed connection fallback) or at least one reachable "direct address"
    /// (IPv4 or IPv6). If none of these are given, establishing a connection is not possible.
    Iroh(EndpointAddr),
}

impl TransportAddress {
    pub fn from_iroh(
        node_id: NodeId,
        relay_url: Option<RelayUrl>,
        direct_addresses: impl IntoIterator<Item = SocketAddr>,
    ) -> Self {
        let transport_addrs = direct_addresses.into_iter().map(TransportAddr::Ip);

        let mut endpoint_addr =
            EndpointAddr::new(from_public_key(node_id)).with_addrs(transport_addrs);

        if let Some(url) = relay_url {
            endpoint_addr = endpoint_addr.with_relay_url(url);
        }

        Self::Iroh(endpoint_addr)
    }
}

impl From<EndpointAddr> for TransportAddress {
    fn from(addr: EndpointAddr) -> Self {
        Self::Iroh(addr)
    }
}

#[derive(Debug, Error)]
pub enum NodeInfoError {
    #[error("missing or invalid signature")]
    InvalidSignature,

    #[error("no addresses given for this transport")]
    MissingTransportAddresses,

    #[error("node id of given transport info does not match")]
    NodeIdMismatch,

    #[error(transparent)]
    Encode(#[from] p2panda_core::cbor::EncodeError),
}

#[cfg(test)]
mod tests {
    use p2panda_core::PrivateKey;

    use super::{NodeInfo, TransportAddress, TransportInfo, UnsignedTransportInfo};

    #[test]
    fn deduplicate_transport_address() {
        let signing_key_1 = PrivateKey::new();
        let node_id_1 = signing_key_1.public_key();

        // De-duplicate addresses when transport is the same.
        let mut info = TransportInfo::new_unsigned();
        info.add_addr(TransportAddress::from_iroh(node_id_1, None, []));
        info.add_addr(TransportAddress::from_iroh(
            node_id_1,
            Some("https://my.relay.net".parse().unwrap()),
            [],
        ));

        assert_eq!(info.len(), 1);
    }

    #[test]
    fn authenticate_address_infos() {
        let signing_key_1 = PrivateKey::new();
        let node_id_1 = signing_key_1.public_key();

        let mut unsigned = UnsignedTransportInfo::new();
        unsigned.add_addr(TransportAddress::from_iroh(
            node_id_1,
            Some("https://my.relay.net".parse().unwrap()),
            [],
        ));

        let info = unsigned.sign(&signing_key_1).unwrap();
        assert!(info.verify(&node_id_1).is_ok());

        // Fails when node id does not match.
        let signing_key_2 = PrivateKey::new();
        let node_id_2 = signing_key_2.public_key();
        assert!(info.verify(&node_id_2).is_err());

        // Fails when information got changed.
        let mut info = info;
        info.addresses.pop().unwrap();
        assert!(info.verify(&node_id_1).is_err());
    }

    #[test]
    fn node_id_mismatch() {
        let signing_key_1 = PrivateKey::new();
        let node_id_1 = signing_key_1.public_key();

        let signing_key_2 = PrivateKey::new();
        let node_id_2 = signing_key_2.public_key();

        // Create transport info for node 1.
        let mut unsigned = UnsignedTransportInfo::new();
        unsigned.add_addr(TransportAddress::from_iroh(
            node_id_1,
            Some("https://my.relay.net".parse().unwrap()),
            [],
        ));
        let transport_info = unsigned.sign(&signing_key_1).unwrap();

        // Create info for node 2 and try to add unrelated transport info.
        let mut node_info = NodeInfo {
            node_id: node_id_2,
            bootstrap: false,
            transports: None,
        };
        assert!(node_info.verify().is_ok());
        assert!(node_info.update_transports(transport_info).is_err());
    }

    #[test]
    fn latest_transport_info_wins() {
        let signing_key_1 = PrivateKey::new();
        let node_id_1 = signing_key_1.public_key();

        // Create "newer" transport info.
        let transport_info_1 = {
            let mut unsigned = UnsignedTransportInfo::new();
            unsigned.add_addr(TransportAddress::from_iroh(
                node_id_1,
                Some("https://my.relay.net".parse().unwrap()),
                [],
            ));
            unsigned.timestamp = 2; // Force "newer" timestamp.
            unsigned.sign(&signing_key_1).unwrap()
        };

        // Create "older" transport info.
        let transport_info_2 = {
            let mut unsigned = UnsignedTransportInfo::new();
            unsigned.add_addr(TransportAddress::from_iroh(
                node_id_1,
                Some("https://my.relay.net".parse().unwrap()),
                [],
            ));
            unsigned.timestamp = 1; // Force "older" timestamp.
            unsigned.sign(&signing_key_1).unwrap()
        };

        // Register both transport infos with node.
        let mut node_info = NodeInfo {
            node_id: node_id_1,
            bootstrap: true,
            transports: None,
        };
        assert!(node_info.verify().is_ok());
        assert!(node_info.update_transports(transport_info_1).is_ok());
        assert!(node_info.update_transports(transport_info_2).is_ok());

        // The "newer" transport info is the only one registered.
        assert_eq!(node_info.transports.as_ref().unwrap().len(), 1);
        assert_eq!(node_info.transports.unwrap().timestamp, 2);
    }
}
