// SPDX-License-Identifier: MIT OR Apache-2.0

mod manager;
mod session;
mod walker;

pub use manager::{DiscoveryManager, DiscoveryMetrics, ToDiscoveryManager};

pub const DISCOVERY_PROTOCOL_ID: &[u8] = b"p2panda/confidential_discovery/v1";
