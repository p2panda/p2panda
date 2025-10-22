// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::Hash;

use crate::NetworkId;

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
pub fn hash_protocol_id_with_network_id(
    protocol_id: impl AsRef<[u8]>,
    network_id: &NetworkId,
) -> [u8; 32] {
    Hash::new([protocol_id.as_ref(), network_id].concat()).into()
}
