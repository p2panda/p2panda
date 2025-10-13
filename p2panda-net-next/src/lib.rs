// SPDX-License-Identifier: MIT OR Apache-2.0

mod actors;
mod addrs;
mod defaults;
mod network;
mod protocols;

pub use network::NetworkBuilder;

pub type TopicId = [u8; 32];

/// Unique 32 byte identifier for a network.
///
/// The network identifier is used to achieve separation and prevent interoperability between
/// distinct networks. This is the most global identifier to group peers into networks. Different
/// applications may choose to share the same underlying network infrastructure by using the same
/// network identifier.
///
/// A bitwise XOR operation is performed against each network protocol identifier which is
/// registered with `p2panda-net`. Even if two instances of `p2panda-net` are created with the same
/// network protocols, any communication attempts will fail if they are not using the same network
/// identifier.
pub type NetworkId = [u8; 32];

/// Converts an `iroh` public key type to the `p2panda-core` implementation.
pub(crate) fn to_public_key(key: iroh_base::PublicKey) -> p2panda_core::PublicKey {
    p2panda_core::PublicKey::from_bytes(key.as_bytes()).expect("already validated public key")
}

/// Converts a `p2panda-core` public key to the "iroh" type.
pub(crate) fn from_public_key(key: p2panda_core::PublicKey) -> iroh_base::PublicKey {
    iroh_base::PublicKey::from_bytes(key.as_bytes()).expect("already validated public key")
}

/// Converts a `p2panda-core` private key to the "iroh" type.
// TODO: Remove once used.
#[allow(dead_code)]
pub(crate) fn from_private_key(key: p2panda_core::PrivateKey) -> iroh_base::SecretKey {
    iroh_base::SecretKey::from_bytes(key.as_bytes())
}
