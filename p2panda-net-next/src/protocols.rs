// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::BTreeMap;

use iroh::protocol::DynProtocolHandler as ProtocolHandler;

use crate::NetworkId;

/// Unique byte identifier for a network protocol.
///
/// The protocol identifier is supplied along with a protocol handler when registering a network
/// protocol with the `NetworkBuilder`.
///
/// A hash function is performed against each network protocol identifier which is registered with
/// `p2panda-net`. Even if two instances of `p2panda-net` are created with the same network
/// protocols, any communication attempts will fail if they are not using the same network
/// identifier.
pub type ProtocolId = [u8];

/// Hash the concatenation of the given protocol and network identifiers (using a `+` delimiter).
pub(crate) fn hash_protocol_id_with_network_id(
    protocol_id: &ProtocolId,
    network_id: &NetworkId,
) -> [u8; 32] {
    let ids = [protocol_id, b"+", network_id].concat();
    let hash = blake3::hash(&ids);

    hash.into()
}

/// Mapping of a hashed protocol identifier to a protocol handler.
///
/// The protocol identifier is hashed with the network identifier before being inserted into the
/// map.
pub(crate) type ProtocolMap = BTreeMap<[u8; 32], Box<dyn ProtocolHandler>>;
