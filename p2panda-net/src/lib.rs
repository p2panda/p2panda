// SPDX-License-Identifier: AGPL-3.0-or-later

mod addrs;
mod bytes;
pub mod config;
mod engine;
pub mod network;
mod protocols;
mod sync;

pub use addrs::{NodeAddress, RelayUrl};
pub use config::Config;
pub use network::{Network, NetworkBuilder, RelayMode};
pub use protocols::ProtocolHandler;
pub use sync::SyncConfiguration;

#[cfg(feature = "log-sync")]
pub use p2panda_sync::log_sync::LogSyncProtocol;

/// A unique 32 byte identifier for a network.
pub type NetworkId = [u8; 32];

/// Topic ids are announced on the network and used to identify peers with overlapping interests.
///
/// Once identified, peers join a gossip overlay and, if a sync protocol has been provided, attempt
/// to synchronize past state before entering "live mode".
pub trait TopicId {
    fn id(&self) -> [u8; 32];
}

pub(crate) fn to_public_key(key: iroh_net::key::PublicKey) -> p2panda_core::PublicKey {
    p2panda_core::PublicKey::from_bytes(key.as_bytes()).expect("already validated public key")
}
