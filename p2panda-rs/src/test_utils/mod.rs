// SPDX-License-Identifier: AGPL-3.0-or-later

//! This module provides tools which can be used for testing.
//!
//! It includes fixtures and templates which can be injected into tests, mock node and client
//! implementations.
pub mod constants;
pub mod fixtures;
pub mod memory_store;

/// Generates random bytes of given length.
pub fn generate_random_bytes(len: usize) -> Vec<u8> {
    (0..len).map(|_| rand::random::<u8>()).collect()
}
