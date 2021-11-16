// SPDX-License-Identifier: AGPL-3.0-or-later

//! # p2panda test-utils
//!
//! This module provides tools used for testing and generating test data for `p2panda-rs` and `p2panda-js`. 
//!
//! It includes:
//! - a mock node
//! - a mock client
//! - methods for generating test data (used in `p2panda-js` tests)

#[cfg(test)]
pub mod fixtures;
pub mod logs;
pub mod node;
pub mod materialiser;
pub mod data;
pub mod client;
mod utils;

pub use utils::*;
