// SPDX-License-Identifier: MIT OR Apache-2.0

mod actors;
mod addrs;
mod args;
mod cbor;
mod config;
mod network;
mod protocols;
mod streams;
#[cfg(test)]
mod test_utils;
mod utils;

pub use addrs::{
    AuthenticatedTransportInfo, NodeId, NodeInfo, NodeInfoError, NodeTransportInfo,
    TransportAddress, TransportInfo, TrustedTransportInfo, UnsignedTransportInfo,
};
#[cfg(feature = "mdns")]
pub use config::MdnsDiscoveryMode;
pub use network::NetworkBuilder;

/// Unique 32 byte identifier for an ephemeral- or eventually-consistent stream topic.
///
/// A topic identifier is required when subscribing or publishing to a stream.
pub type TopicId = [u8; 32];

/// Unique 32 byte identifier for a network.
///
/// The network identifier is used to achieve separation and prevent interoperability between
/// distinct networks. This is the most global identifier to group peers into networks. Different
/// applications may choose to share the same underlying network infrastructure by using the same
/// network identifier.
///
/// It is highly recommended to use a cryptographically secure pseudorandom number generator
/// (CSPRNG) when generating a network identifier.
///
/// A blake3 hash function is performed against each protocol identifier which is registered
/// with `p2panda-net`. Even if two instances of `p2panda-net` are created with the same network
/// protocols, any communication attempts will fail if they are not using the same network
/// identifier.
pub type NetworkId = [u8; 32];
