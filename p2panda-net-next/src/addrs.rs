// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::BTreeSet;
use std::collections::HashSet;
use std::hash::Hash as StdHash;
use std::hash::Hasher;
use std::net::SocketAddr;

use p2panda_core::cbor::encode_cbor;
use p2panda_core::{PrivateKey, Signature};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::from_public_key;
use crate::{current_timestamp, to_public_key};

/// Default STUN port used by the relay server.
///
/// The STUN port as defined by [RFC 8489](<https://www.rfc-editor.org/rfc/rfc8489#section-18.6>)
// TODO: Remove once used.
#[allow(dead_code)]
pub const DEFAULT_STUN_PORT: u16 = 3478;

pub type NodeId = p2panda_core::PublicKey;

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
    pub transports: Option<NodeTransportInfo>,
}

impl NodeInfo {
    pub fn update(mut self, other: NodeInfo) -> NodeInfo {
        assert_eq!(self.node_id, other.node_id);

        // Choose "latest" node info by checking timestamp if given.
        match (self.transports.as_ref(), other.transports) {
            (None, None) => {
                // Nothing to do.
            }
            (None, Some(other)) => {
                self.transports = Some(other);
            }
            (Some(_), None) => {
                // Nothing to do.
            }
            (Some(current), Some(other)) => {
                if other.timestamp > current.timestamp {
                    self.transports = Some(other);
                }
            }
        }

        self
    }

    pub fn verify(&self) -> Result<(), NodeInfoError> {
        match self.transports {
            Some(ref transports) => transports.verify(self.node_id),
            None => Ok(()),
        }
    }
}

impl From<iroh::NodeAddr> for NodeInfo {
    fn from(addr: iroh::NodeAddr) -> Self {
        NodeInfo {
            node_id: to_public_key(addr.node_id),
            bootstrap: false,
            transports: Some(NodeTransportInfo {
                timestamp: current_timestamp(),
                addresses: HashSet::from([TransportAddress::from(addr)]),
                signature: None,
            }),
        }
    }
}

impl TryFrom<NodeInfo> for iroh::NodeAddr {
    type Error = NodeInfoError;

    fn try_from(node_info: NodeInfo) -> Result<Self, Self::Error> {
        let node_id = from_public_key(node_info.node_id);

        let Some(transports) = node_info.transports else {
            return Err(NodeInfoError::MissingTransportAddresses);
        };

        let result = transports
            .addresses
            .into_iter()
            .find_map(|address| match address {
                TransportAddress::Iroh {
                    relay_url,
                    direct_addresses,
                } => {
                    let mut node_addr = iroh::NodeAddr::new(node_id)
                        .with_direct_addresses(direct_addresses.into_iter());
                    if let Some(url) = relay_url {
                        node_addr = node_addr.with_relay_url(url);
                    }
                    Some(node_addr)
                }
                #[allow(unreachable_patterns)]
                _ => None,
            });

        result.ok_or(NodeInfoError::MissingTransportAddresses)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeTransportInfo {
    /// UNIX timestamp from when this transport information was published.
    ///
    /// This can be used to find out which information is the "latest".
    pub timestamp: u64,

    /// Associated transport addresses to aid establishing a connection to this node.
    pub addresses: HashSet<TransportAddress>,

    /// Signature to proof authenticity of this node id.
    ///
    /// Other nodes can validate the authenticity by checking this signature against the associated
    /// node id and info.
    ///
    /// This protects against attacks where nodes maliciously publish wrong information about other
    /// nodes, for example to make them unreachable due to invalid addresses.
    pub signature: Option<Signature>,
}

impl NodeTransportInfo {
    fn to_signable_bytes(&mut self) -> Result<Vec<u8>, NodeInfoError> {
        self.signature = None;
        let bytes = encode_cbor(&self)?;
        Ok(bytes)
    }

    pub fn sign(mut self, private_key: &PrivateKey) -> Result<NodeTransportInfo, NodeInfoError> {
        let bytes = self.to_signable_bytes()?;
        self.signature = Some(private_key.sign(&bytes));
        Ok(self)
    }

    pub fn verify(&self, node_id: NodeId) -> Result<(), NodeInfoError> {
        match self.signature {
            Some(signature) => {
                let bytes = self.clone().to_signable_bytes()?;
                if !node_id.verify(&bytes, &signature) {
                    Err(NodeInfoError::InvalidSignature)
                } else {
                    Ok(())
                }
            }
            None => Err(NodeInfoError::InvalidSignature),
        }
    }
}

/// Associated transport addresses to aid establishing a connection to this node.
///
/// Currently this only supports using iroh (Internet Protocol) to connect.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TransportAddress {
    /// Information to connect to another node via QUIC / UDP / IP using iroh for holepunching and
    /// relayed connections as a fallback.
    ///
    /// To connect to another node either their home relay needs to be known (to coordinate
    /// holepunching or relayed connection fallback) or at least one reachable direct address (IPv4
    /// or IPv6). If none of these are given, establishing a connection is not possible.
    Iroh {
        /// Current iroh home relay of this node.
        ///
        /// If `None` this node doesn't use a relay currently. Either because they are attempting
        /// connection to one or they don't have one configured.
        relay_url: Option<iroh::RelayUrl>,

        /// Direct addresses (IPv4 or IPv6) we can use to directly connect to this node using iroh.
        direct_addresses: BTreeSet<SocketAddr>,
    },
}

impl From<iroh::NodeAddr> for TransportAddress {
    fn from(addr: iroh::NodeAddr) -> Self {
        Self::Iroh {
            relay_url: addr.relay_url,
            direct_addresses: addr.direct_addresses,
        }
    }
}

// Only hash the "key" (discriminant) of the enum, inserting `TransportAddress` into a hash map or
// -set will overwrite existing values of the same key.
impl StdHash for TransportAddress {
    fn hash<H: Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
    }
}

// @TODO: This is weird. Maybe we should not use hash sets and have the "update" method handle
// duplicate transports manually.
impl PartialEq for TransportAddress {
    fn eq(&self, other: &Self) -> bool {
        core::mem::discriminant(self) == core::mem::discriminant(other)
    }
}

impl Eq for TransportAddress {}

#[derive(Debug, Error)]
pub enum NodeInfoError {
    #[error("missing or invalid signature")]
    InvalidSignature,

    #[error("no addresses given for this transport")]
    MissingTransportAddresses,

    #[error(transparent)]
    Encode(#[from] p2panda_core::cbor::EncodeError),
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashSet};

    use super::TransportAddress;

    #[test]
    fn transport_address_discriminant_hash() {
        let address_1 = TransportAddress::Iroh {
            relay_url: None,
            direct_addresses: BTreeSet::default(),
        };

        let address_2 = TransportAddress::Iroh {
            relay_url: Some("https://my.relay.org".parse().unwrap()),
            direct_addresses: BTreeSet::default(),
        };

        let mut map = HashSet::new();
        map.insert(address_1);
        map.insert(address_2);

        // When inserted in a hash set the value gets overwritten to assure uniqueness over the
        // transport type / enum discriminant.
        assert_eq!(map.len(), 1);
    }
}
