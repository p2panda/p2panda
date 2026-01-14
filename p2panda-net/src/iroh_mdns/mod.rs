// SPDX-License-Identifier: MIT OR Apache-2.0

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
