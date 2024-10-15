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

// This is used in the construction of the shared `AbortOnDropHandle`.
pub type JoinErrToStr = Box<dyn Fn(tokio::task::JoinError) -> String + Send + Sync + 'static>;

pub type NetworkId = [u8; 32];
pub type TopicId = [u8; 32];
