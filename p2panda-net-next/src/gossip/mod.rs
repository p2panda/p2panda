// SPDX-License-Identifier: MIT OR Apache-2.0

mod actors;
mod api;
mod builder;
mod events;
#[cfg(test)]
mod tests;

#[cfg(test)]
pub use actors::{GossipManagerState, ToGossipSession};
pub use api::{EphemeralStream, EphemeralStreamError, EphemeralSubscription, Gossip, GossipError};
pub use builder::Builder;
pub use events::GossipEvent;
