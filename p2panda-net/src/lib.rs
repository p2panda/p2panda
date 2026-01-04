// SPDX-License-Identifier: MIT OR Apache-2.0

#[cfg(feature = "address_book")]
pub mod address_book;
pub mod addrs;
pub mod cbor;
#[cfg(feature = "confidential_discovery")]
pub mod discovery;
#[cfg(feature = "gossip")]
pub mod gossip;
#[cfg(feature = "iroh_endpoint")]
pub mod iroh_endpoint;
#[cfg(feature = "iroh_mdns")]
pub mod iroh_mdns;
#[cfg(feature = "log_sync")]
pub mod log_sync;
#[cfg(feature = "supervisor")]
pub mod supervisor;
#[cfg(any(test, feature = "test_utils"))]
pub mod test_utils;
pub mod timestamp;
pub mod utils;
pub mod watchers;

#[cfg(feature = "address_book")]
pub use address_book::AddressBook;
#[cfg(feature = "confidential_discovery")]
pub use discovery::Discovery;
#[cfg(feature = "gossip")]
pub use gossip::Gossip;
#[cfg(feature = "iroh_endpoint")]
pub use iroh_endpoint::Endpoint;
#[cfg(feature = "iroh_mdns")]
pub use iroh_mdns::MdnsDiscovery;
#[cfg(feature = "log_sync")]
pub use log_sync::LogSync;

pub type NodeId = p2panda_core::PublicKey;

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
/// A blake3 hash function is performed against each protocol identifier which is registered with
/// `p2panda-net`. Even if two instances of `p2panda-net` are created with the same network
/// protocols, any communication attempts will fail if they are not using the same network
/// identifier.
pub type NetworkId = [u8; 32];

/// Unique byte identifier for a network protocol.
///
/// The protocol identifier is supplied along with a protocol handler when registering a network
/// protocol.
///
/// A hash function is performed against each network protocol identifier which is registered with
/// `p2panda-net`. Even if two instances of `p2panda-net` are created with the same network
/// protocols, any communication attempts will fail if they are not using the same network
/// identifier.
pub type ProtocolId = Vec<u8>;

/// Hash the concatenation of the given protocol- and network identifiers.
fn hash_protocol_id_with_network_id(
    protocol_id: impl AsRef<[u8]>,
    network_id: NetworkId,
) -> Vec<u8> {
    p2panda_core::Hash::new([protocol_id.as_ref(), &network_id].concat())
        .as_bytes()
        .to_vec()
}
