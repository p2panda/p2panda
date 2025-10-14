// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::BTreeMap;

use iroh::protocol::DynProtocolHandler as ProtocolHandler;

use crate::NetworkId;

/// Unique byte identifier for a network protocol.
///
/// The protocol identifier is supplied along with a protocol handler when registering a network
/// protocol with the `NetworkBuilder`.
///
/// A bitwise XOR operation is performed against each network protocol identifier which is
/// registered with `p2panda-net`. Even if two instances of `p2panda-net` are created with the same
/// network protocols, any communication attempts will fail if they are not using the same network
/// identifier.
pub type ProtocolId = [u8];

/// Perform a bitwise XOR of the protocol identifier and network identifier.
///
/// This is used to enforce network separation, so that two nodes running with the same protocol(s)
/// but different network IDs will not exchange data.
pub(crate) fn protocol_id_xor(protocol_id: &ProtocolId, network_id: NetworkId) -> Vec<u8> {
    let mut xor: Vec<u8> = protocol_id
        .iter()
        .zip(network_id.iter())
        .map(|(x, y)| *x ^ *y)
        .collect();

    // Extend the vector to a minimum of 32 bytes.
    if xor.len() < network_id.len() {
        xor.extend(&network_id[xor.len()..]);
    }

    xor
}

/// Mapping of an XOR'd protocol identifier to a protocol handler.
///
/// The protocol identifier is XOR'd with the network identifier before being inserted into the
/// map.
pub(crate) type ProtocolMap = BTreeMap<Vec<u8>, Box<dyn ProtocolHandler>>;

#[cfg(test)]
mod tests {
    use super::protocol_id_xor;

    #[test]
    fn xor_id_output_is_correct() {
        let protocol_id = [1; 32];
        let network_id = [2; 32];

        let protocol_id_xor = protocol_id_xor(&protocol_id, network_id);

        assert_eq!(protocol_id_xor.len(), 32);
        assert_eq!(protocol_id_xor, [3; 32].to_vec());
        assert_ne!(protocol_id_xor, protocol_id);
    }

    #[test]
    fn xor_id_length_is_correct() {
        // This protocol ID is not 32 bytes in length (it's much shorter).
        // We want to ensure that the output of the XOR function is 32 bytes.
        let protocol_id = b"/p2panda/1";
        let network_id = [2; 32];

        let protocol_id_xor = protocol_id_xor(protocol_id, network_id);

        assert_eq!(protocol_id_xor.len(), 32);
    }
}
