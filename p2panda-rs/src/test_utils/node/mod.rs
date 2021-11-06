// SPDX-License-Identifier: AGPL-3.0-or-later

//! Mock p2panda node, related data types and utilities.
//! 
//! Only to be used in a testing environment!

mod node;
pub mod utils;

pub use node::{send_to_node, Node, Database};
