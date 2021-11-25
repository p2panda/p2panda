// SPDX-License-Identifier: AGPL-3.0-or-later

//! # p2panda test-utils
//!
//! This module provides tools used which can be used for testing and generating test data for `p2panda-rs` and `p2panda-js`. 
//!
//! It includes:
//! - fixtures and templates which can be injected into tests
//! - mock node and client with experimental materialisation logic implemented
//! - methods for generating test data (used in `p2panda-js` tests)

pub mod fixtures;
pub mod test_data;
pub mod mocks;
mod utils;

pub use utils::*;