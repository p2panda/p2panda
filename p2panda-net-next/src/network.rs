// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::PublicKey;

/// An event to be broadcast to the network.
#[derive(Clone, Debug)]
pub enum ToNetwork {
    Message { bytes: Vec<u8> },
}

/// An event received from the network.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FromNetwork {
    GossipMessage {
        bytes: Vec<u8>,
        delivered_from: PublicKey,
    },
    SyncMessage {
        header: Vec<u8>,
        payload: Option<Vec<u8>>,
        delivered_from: PublicKey,
    },
}
