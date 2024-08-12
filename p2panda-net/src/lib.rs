// SPDX-License-Identifier: AGPL-3.0-or-later

mod addrs;
pub mod config;
pub mod discovery;
mod engine;
mod handshake;
mod message;
pub mod network;
mod protocols;

pub use addrs::{NodeAddress, RelayUrl};
pub use config::Config;
#[cfg(feature = "mdns")]
pub use discovery::mdns::LocalDiscovery;
pub use message::{FromBytes, ToBytes};
pub use network::{Network, NetworkBuilder, RelayMode};
pub use protocols::ProtocolHandler;

pub type NetworkId = [u8; 32];

pub type TopicId = [u8; 32];
