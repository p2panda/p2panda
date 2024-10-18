// SPDX-License-Identifier: AGPL-3.0-or-later

mod addrs;
pub mod config;
mod engine;
mod message;
pub mod network;
mod protocols;
mod sync;

pub use addrs::{NodeAddress, RelayUrl};
pub use config::Config;
pub use message::{FromBytes, ToBytes};
pub use network::{Network, NetworkBuilder, RelayMode};
pub use protocols::ProtocolHandler;
pub use tokio_util::task::AbortOnDropHandle;

#[cfg(feature = "log-sync")]
pub use p2panda_sync::log_sync::LogSyncProtocol;

pub type NetworkId = [u8; 32];
pub type TopicId = [u8; 32];
