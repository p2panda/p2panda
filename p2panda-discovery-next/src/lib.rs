// SPDX-License-Identifier: MIT OR Apache-2.0

// @TODO: Move address book into `p2panda-store` when crate is ready.
pub mod address_book;
#[cfg(any(test, feature = "test_utils"))]
pub mod naive;
#[cfg(feature = "random_walk")]
pub mod random_walk;
#[cfg(any(test, feature = "test_utils"))]
pub mod test_utils;
#[cfg(test)]
pub mod tests;
pub mod traits;

pub use traits::{DiscoveryProtocol, DiscoveryResult, DiscoveryStrategy, Receiver, Sender};
