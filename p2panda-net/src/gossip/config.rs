// SPDX-License-Identifier: MIT OR Apache-2.0

/// The default maximum size in bytes for a gossip message.
pub const DEFAULT_MAX_MESSAGE_SIZE: usize = 4096;

pub type HyParViewConfig = iroh_gossip::proto::HyparviewConfig;

pub type PlumTreeConfig = iroh_gossip::proto::PlumtreeConfig;

#[derive(Clone, Debug)]
pub struct GossipConfig {
    /// Configuration for the swarm membership layer.
    pub membership: HyParViewConfig,

    /// Configuration for the gossip broadcast layer.
    pub broadcast: PlumTreeConfig,

    /// Max message size in bytes.
    ///
    /// This size should be the same across a network to ensure all nodes can transmit and read
    /// large messages.
    ///
    /// At minimum, this size should be large enough to send gossip control messages.
    pub max_message_size: usize,
}

impl Default for GossipConfig {
    fn default() -> Self {
        Self {
            membership: Default::default(),
            broadcast: Default::default(),
            max_message_size: DEFAULT_MAX_MESSAGE_SIZE,
        }
    }
}
