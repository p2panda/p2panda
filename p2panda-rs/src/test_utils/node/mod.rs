// SPDX-License-Identifier: AGPL-3.0-or-later

//! Mock p2panda node, related data types and utilities.
//! 
//! This node mocks functionality which would be implemented in a real world p2panda node. 
//! It does so in a simplistic manner and should only be used in a testing environment or demo 
//! environment.

mod node;
pub mod utils;

pub use node::{send_to_node, Node, Database};
