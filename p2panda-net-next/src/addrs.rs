// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Display;
use std::hash::Hash as StdHash;
use std::mem;
#[cfg(test)]
use std::net::SocketAddr;

use p2panda_core::cbor::encode_cbor;
use p2panda_core::{PrivateKey, Signature};
use p2panda_discovery::address_book;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[cfg(test)]
use crate::utils::from_public_key;
use crate::utils::{current_timestamp, to_public_key};

pub type NodeId = p2panda_core::PublicKey;

#[derive(Clone, Debug, PartialEq, Eq)]
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

    /// Use this node as a "bootstrap".
    pub fn bootstrap(mut self) -> Self {
        self.bootstrap = true;
        self
    }

    /// Updates transport info for a node if it is newer ("last-write wins" principle).
    ///
    /// Returns true if given transport info is newer than the current one.
    pub fn update_transports(&mut self, other: TransportInfo) -> Result<bool, NodeInfoError> {
        other.verify(&self.node_id)?;

        // Choose "latest" info by checking timestamp if given.
        let mut is_newer = false;
        match self.transports.as_ref() {
            None => {
                is_newer = true;
                self.transports = Some(other)
            }
            Some(current) => {
                if other.timestamp() > current.timestamp() {
                    self.transports = Some(other);
                    is_newer = true;
                }
            }
        }

        Ok(is_newer)
    }

    pub fn verify(&self) -> Result<(), NodeInfoError> {
        match self.transports {
            Some(ref transports) => transports.verify(&self.node_id),
            None => Ok(()),
        }
    }
}

impl TryFrom<NodeInfo> for iroh::EndpointAddr {
    type Error = NodeInfoError;

    fn try_from(node_info: NodeInfo) -> Result<Self, Self::Error> {
        let Some(transports) = node_info.transports else {
            return Err(NodeInfoError::MissingTransportAddresses);
        };

        transports
            .addresses()
            .iter()
            .find_map(|address| match address {
                TransportAddress::Iroh(endpoint_addr) => Some(endpoint_addr),
                #[allow(unreachable_patterns)]
                _ => None,
            })
            .cloned()
            .ok_or(NodeInfoError::MissingTransportAddresses)
    }
}

impl From<iroh::EndpointAddr> for NodeInfo {
    fn from(addr: iroh::EndpointAddr) -> Self {
        let node_id = to_public_key(addr.id);
        let transports = TransportInfo::from(TrustedTransportInfo::from(addr));

        Self {
            node_id,
            bootstrap: false,
            transports: Some(transports),
        }
    }
}

impl address_book::NodeInfo<NodeId> for NodeInfo {
    type Transports = AuthenticatedTransportInfo;

    fn id(&self) -> NodeId {
        self.node_id
    }

    fn is_bootstrap(&self) -> bool {
        self.bootstrap
    }

    fn transports(&self) -> Option<Self::Transports> {
        match &self.transports {
            Some(TransportInfo::Authenticated(info)) => Some(info.clone()),
            Some(TransportInfo::Trusted(_)) => {
                // "Trusted" information is _not_ authenticated and can not be used for discovery
                // services as the origin of the data can't be verified by other parties.
                None
            }
            None => None,
        }
    }
}

pub trait NodeTransportInfo {
    /// Returns UNIX timestamp from when this information was created.
    fn timestamp(&self) -> u64;

    /// Returns all associated transport addresses.
    fn addresses(&self) -> Vec<TransportAddress>;

    /// Returns number of associated transports for this node.
    fn len(&self) -> usize;

    /// Returns `false` if no transports are given.
    fn is_empty(&self) -> bool;

    /// Check authenticity integrity of this information when possible.
    fn verify(&self, node_id: &NodeId) -> Result<(), NodeInfoError>;
}

/// Transport protocols information we can use to connect to a node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransportInfo {
    /// Unauthenticated transport info we "trust" to be correct since it came to us via an verified
    /// side-channel (scanning QR code, sharing in a trusted chat group etc.).
    ///
    /// This info is never shared across the network services and is only used _locally_ by our own
    /// node. See `AuthenticatedTransportInfo` for an alternative which can be automatically
    /// distributed.
    Trusted(TrustedTransportInfo),

    /// Signed transport info which can be automatically shared across the network by discovery
    /// services and "untrusted" intermediaries since the original author is verifiable.
    Authenticated(AuthenticatedTransportInfo),
}

impl TransportInfo {
    pub fn new_trusted() -> TrustedTransportInfo {
        TrustedTransportInfo::new()
    }

    pub fn new_unsigned() -> UnsignedTransportInfo {
        UnsignedTransportInfo::new()
    }
}

impl NodeTransportInfo for TransportInfo {
    fn timestamp(&self) -> u64 {
        match self {
            TransportInfo::Trusted(info) => info.timestamp(),
            TransportInfo::Authenticated(info) => info.timestamp(),
        }
    }

    fn addresses(&self) -> Vec<TransportAddress> {
        match self {
            TransportInfo::Trusted(info) => info.addresses(),
            TransportInfo::Authenticated(info) => info.addresses(),
        }
    }

    fn len(&self) -> usize {
        match self {
            TransportInfo::Trusted(info) => info.addresses.len(),
            TransportInfo::Authenticated(info) => info.addresses.len(),
        }
    }

    fn is_empty(&self) -> bool {
        match self {
            TransportInfo::Trusted(info) => info.addresses.is_empty(),
            TransportInfo::Authenticated(info) => info.addresses.is_empty(),
        }
    }

    fn verify(&self, node_id: &NodeId) -> Result<(), NodeInfoError> {
        match self {
            TransportInfo::Trusted(info) => info.verify(node_id),
            TransportInfo::Authenticated(info) => info.verify(node_id),
        }
    }
}

impl Display for TransportInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportInfo::Trusted(info) => write!(f, "{info}"),
            TransportInfo::Authenticated(info) => write!(f, "{info}"),
        }
    }
}

impl From<iroh::EndpointAddr> for TransportInfo {
    fn from(addr: iroh::EndpointAddr) -> Self {
        Self::from(TrustedTransportInfo::from(addr))
    }
}

impl From<AuthenticatedTransportInfo> for TransportInfo {
    fn from(value: AuthenticatedTransportInfo) -> Self {
        Self::Authenticated(value)
    }
}

impl From<TrustedTransportInfo> for TransportInfo {
    fn from(value: TrustedTransportInfo) -> Self {
        Self::Trusted(value)
    }
}

/// Signed transport info which can be automatically shared across the network by discovery
/// services and "untrusted" intermediaries since the original author is verifiable.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthenticatedTransportInfo {
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

impl AuthenticatedTransportInfo {
    pub fn new_unsigned() -> UnsignedTransportInfo {
        UnsignedTransportInfo::new()
    }

    fn to_unsigned(&self) -> UnsignedTransportInfo {
        UnsignedTransportInfo {
            timestamp: self.timestamp,
            addresses: self.addresses.clone(),
        }
    }
}

impl NodeTransportInfo for AuthenticatedTransportInfo {
    fn timestamp(&self) -> u64 {
        self.timestamp
    }

    fn addresses(&self) -> Vec<TransportAddress> {
        self.addresses.clone()
    }

    fn len(&self) -> usize {
        self.addresses.len()
    }

    fn is_empty(&self) -> bool {
        self.addresses.is_empty()
    }

    fn verify(&self, node_id: &NodeId) -> Result<(), NodeInfoError> {
        let bytes = self.to_unsigned().to_bytes()?;

        if !node_id.verify(&bytes, &self.signature) {
            Err(NodeInfoError::InvalidSignature)
        } else {
            Ok(())
        }
    }
}

impl Display for AuthenticatedTransportInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let addresses = if self.addresses.is_empty() {
            "[]".to_string()
        } else {
            self.addresses.iter().map(|addr| addr.to_string()).collect()
        };

        write!(
            f,
            "[authenticated] timestamp={}, addresses={}",
            self.timestamp, addresses
        )
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

impl Default for UnsignedTransportInfo {
    fn default() -> Self {
        Self::new()
    }
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
    pub fn sign(
        self,
        signing_key: &PrivateKey,
    ) -> Result<AuthenticatedTransportInfo, NodeInfoError> {
        Ok(AuthenticatedTransportInfo {
            timestamp: self.timestamp,
            signature: {
                let bytes = self.to_bytes()?;
                signing_key.sign(&bytes)
            },
            addresses: self.addresses,
        })
    }
}

impl From<iroh::EndpointAddr> for UnsignedTransportInfo {
    fn from(addr: iroh::EndpointAddr) -> Self {
        Self::from_addrs([addr.into()])
    }
}

/// Unauthenticated transport info we "trust" to be correct since it came to us via an verified
/// side-channel (scanning QR code, sharing in a trusted chat group etc.).
///
/// This info is never shared across the network services and is only used _locally_ by our own
/// node. See `AuthenticatedTransportInfo` for an alternative which can be automatically
/// distributed.
#[derive(Clone, Debug, PartialEq, Eq, StdHash, Serialize, Deserialize)]
pub struct TrustedTransportInfo {
    /// UNIX timestamp from when this transport information was published.
    ///
    /// This can be used to find out which information is the "latest".
    pub timestamp: u64,

    /// Associated transport addresses to aid establishing a connection to this node.
    pub addresses: Vec<TransportAddress>,
}

impl Default for TrustedTransportInfo {
    fn default() -> Self {
        Self::new()
    }
}

impl TrustedTransportInfo {
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
}

impl NodeTransportInfo for TrustedTransportInfo {
    fn timestamp(&self) -> u64 {
        self.timestamp
    }

    fn addresses(&self) -> Vec<TransportAddress> {
        self.addresses.clone()
    }

    fn len(&self) -> usize {
        self.addresses.len()
    }

    fn is_empty(&self) -> bool {
        self.addresses.is_empty()
    }

    fn verify(&self, node_id: &NodeId) -> Result<(), NodeInfoError> {
        for address in &self.addresses {
            address.verify(node_id)?;
        }

        Ok(())
    }
}

impl From<iroh::EndpointAddr> for TrustedTransportInfo {
    fn from(addr: iroh::EndpointAddr) -> Self {
        Self::from_addrs([addr.into()])
    }
}

impl Display for TrustedTransportInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let addresses = if self.addresses.is_empty() {
            "[]".to_string()
        } else {
            self.addresses.iter().map(|addr| addr.to_string()).collect()
        };

        write!(
            f,
            "[trusted] timestamp={}, addresses={}",
            self.timestamp, addresses
        )
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
    Iroh(iroh::EndpointAddr),
}

impl TransportAddress {
    #[cfg(test)]
    pub fn from_iroh(
        node_id: NodeId,
        relay_url: Option<iroh::RelayUrl>,
        direct_addresses: impl IntoIterator<Item = SocketAddr>,
    ) -> Self {
        let transport_addrs = direct_addresses.into_iter().map(iroh::TransportAddr::Ip);

        let mut endpoint_addr =
            iroh::EndpointAddr::new(from_public_key(node_id)).with_addrs(transport_addrs);

        if let Some(url) = relay_url {
            endpoint_addr = endpoint_addr.with_relay_url(url);
        }

        Self::Iroh(endpoint_addr)
    }

    pub fn verify(&self, node_id: &NodeId) -> Result<(), NodeInfoError> {
        // Make sure the given address matches the node id.
        #[allow(irrefutable_let_patterns)]
        if let TransportAddress::Iroh(endpoint_addr) = self
            && &to_public_key(endpoint_addr.id) != node_id
        {
            return Err(NodeInfoError::NodeIdMismatch);
        }

        Ok(())
    }
}

impl From<iroh::EndpointAddr> for TransportAddress {
    fn from(addr: iroh::EndpointAddr) -> Self {
        Self::Iroh(addr)
    }
}

impl Display for TransportAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportAddress::Iroh(endpoint_addr) => {
                write!(f, "[iroh] {:?}", endpoint_addr.addrs)
            }
        }
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

    use crate::addrs::NodeTransportInfo;

    use super::{AuthenticatedTransportInfo, NodeInfo, TransportAddress, UnsignedTransportInfo};

    #[test]
    fn deduplicate_transport_address() {
        let signing_key_1 = PrivateKey::new();
        let node_id_1 = signing_key_1.public_key();

        // De-duplicate addresses when transport is the same.
        let mut info = AuthenticatedTransportInfo::new_unsigned();
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
        assert!(node_info.update_transports(transport_info.into()).is_err());
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
        assert!(node_info.update_transports(transport_info_1.into()).is_ok());
        assert!(node_info.update_transports(transport_info_2.into()).is_ok());

        // The "newer" transport info is the only one registered.
        assert_eq!(node_info.transports.as_ref().unwrap().len(), 1);
        assert_eq!(node_info.transports.unwrap().timestamp(), 2);
    }
}
