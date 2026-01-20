// SPDX-License-Identifier: MIT OR Apache-2.0

//! Resolve transport information for nearby nodes on the local-area network via multicast DNS
//! (mDNS).
mod actor;
mod api;
mod builder;
mod config;
#[cfg(feature = "supervisor")]
mod supervisor;
#[cfg(test)]
mod tests;

pub use api::{MdnsDiscovery, MdnsDiscoveryError};
pub use builder::Builder;
pub use config::MdnsDiscoveryMode;
