// SPDX-License-Identifier: MIT OR Apache-2.0

#[allow(clippy::module_inception)]
mod discovery;

pub use discovery::{DISCOVERY, Discovery, DiscoveryState, ToDiscovery};
