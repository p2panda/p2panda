// SPDX-License-Identifier: AGPL-3.0-or-later

//! Mock p2panda node and client for demo and testing purposes.
mod client;
mod node;
pub mod utils;

pub use client::Client;
pub use node::{send_to_node, Node};
