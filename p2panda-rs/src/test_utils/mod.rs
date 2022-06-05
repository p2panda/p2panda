// SPDX-License-Identifier: AGPL-3.0-or-later

//! This module provides tools which can be used for testing and generating test data.
//!
//! It includes fixtures and templates which can be injected into tests, mock node and client
//! implementations, methods for generating test data (used in `p2panda-js`).
pub mod constants;
#[cfg(test)]
pub mod fixtures;
pub mod mocks;
pub mod test_data;
pub mod utils;
