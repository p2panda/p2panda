// SPDX-License-Identifier: AGPL-3.0-or-later

mod addrs;
pub mod config;
mod engine;
mod message;
pub mod network;
mod protocols;
mod sync;

use std::fmt::{Debug, Display};

pub use addrs::{NodeAddress, RelayUrl};
pub use config::Config;
pub use message::{FromBytes, ToBytes};
pub use network::{Network, NetworkBuilder, RelayMode};
use p2panda_core::Hash;
pub use protocols::ProtocolHandler;
pub use tokio_util::task::AbortOnDropHandle;

#[cfg(feature = "log-sync")]
pub use p2panda_sync::log_sync::LogSyncProtocol;

pub type NetworkId = [u8; 32];

/// Topics are identified by a network-unique id encoded as a `[u8; 32]`.
/// Topic ids are announced on the network and used to identify peers with
/// similar interests. Once identified, peers join a gossip overlay and, if
/// a sync protocol has been provided, attempt to synchronize past state.
///
/// The `Topic` trait must be implemented on any user defined topic types.
pub trait Topic: Clone + Debug + Display + Send + Sync {
    fn id(&self) -> [u8; 32];
}

impl<T> Topic for T
where
    T: Clone + Debug + Display + Send + Sync,
{
    fn id(&self) -> [u8; 32] {
        Hash::new(self.to_string().as_bytes()).as_bytes().to_owned()
    }
}
