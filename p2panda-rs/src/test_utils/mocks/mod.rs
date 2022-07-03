// SPDX-License-Identifier: AGPL-3.0-or-later

//! Mock p2panda node and client for demo and testing purposes.
mod client;
#[cfg(not(target_arch = "wasm32"))]
mod node;
pub mod utils;

pub use client::Client;
#[cfg(not(target_arch = "wasm32"))]
pub use node::{send_to_node, Node};
