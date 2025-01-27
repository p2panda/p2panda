// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;

use p2panda_core::PublicKey;
use tracing::debug;

#[derive(Debug, Default)]
pub struct GossipBuffer {
    buffers: HashMap<(PublicKey, [u8; 32]), Vec<Vec<u8>>>,
    counters: HashMap<(PublicKey, [u8; 32]), usize>,
}

impl GossipBuffer {
    pub fn lock(&mut self, peer: PublicKey, topic_id: [u8; 32]) {
        let counter = self.counters.entry((peer, topic_id)).or_default();
        *counter += 1;

        self.buffers.entry((peer, topic_id)).or_default();

        debug!(
            "lock gossip buffer with {} on topic {:?}: {}",
            peer, topic_id, counter
        );
    }

    pub fn unlock(&mut self, peer: PublicKey, topic_id: [u8; 32]) -> usize {
        match self.counters.get_mut(&(peer, topic_id)) {
            Some(counter) => {
                *counter -= 1;
                debug!(
                    "unlock gossip buffer with {} on topic {:?}: {}",
                    peer, topic_id, counter
                );
                *counter
            }
            None => panic!("attempted to unlock non-existing gossip buffer"),
        }
    }

    pub fn drain(&mut self, peer: PublicKey, topic_id: [u8; 32]) -> Option<Vec<Vec<u8>>> {
        self.buffers.remove(&(peer, topic_id))
    }

    pub fn buffer(&mut self, peer: PublicKey, topic_id: [u8; 32]) -> Option<&mut Vec<Vec<u8>>> {
        self.buffers.get_mut(&(peer, topic_id))
    }
}
