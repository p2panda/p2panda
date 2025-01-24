// SPDX-License-Identifier: MIT OR Apache-2.0

//! System events API.
use p2panda_core::PublicKey;

/// Network system events.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum SystemEvent<T> {
    /// Joined a gossip topic via a connection to the given peer(s).
    GossipJoined {
        topic_id: [u8; 32],
        peers: Vec<PublicKey>,
    },

    /// Left a gossip topic.
    // @TODO: This requires `unsubscribe()` to be implemented.
    // https://github.com/p2panda/p2panda/issues/639
    GossipLeft { topic_id: [u8; 32] },

    /// Established a connection with a neighbor.
    GossipNeighborUp { topic_id: [u8; 32], peer: PublicKey },

    /// Lost a connection to a neighbor.
    ///
    /// This event will be emitted approximately 30 seconds after the connection is lost.
    GossipNeighborDown { topic_id: [u8; 32], peer: PublicKey },

    /// Discovered a new peer in the network.
    PeerDiscovered { peer: PublicKey },

    /// Started a sync session.
    SyncStarted { topic: Option<T>, peer: PublicKey },

    /// Completed a sync session.
    SyncDone { topic: T, peer: PublicKey },

    /// Failed to complete a sync session.
    SyncFailed { topic: Option<T>, peer: PublicKey },
}
