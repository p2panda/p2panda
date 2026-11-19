// SPDX-License-Identifier: MIT OR Apache-2.0

//! Gossip protocol to broadcast ephemeral messages to all online nodes interested in the same
//! topic.
mod actors;
mod api;
mod builder;
mod config;
mod events;
#[cfg(test)]
mod tests;

pub use api::{Gossip, GossipError, GossipHandle, GossipPublishError, GossipSubscription};
pub use builder::Builder;
pub use config::{BroadcastConfig, DEFAULT_MAX_MESSAGE_SIZE, GossipConfig, MembershipConfig};
pub use events::GossipEvent;
