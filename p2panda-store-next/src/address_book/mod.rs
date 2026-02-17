// SPDX-License-Identifier: MIT OR Apache-2.0

#[cfg(feature = "memory")]
mod memory;
#[cfg(any(test, feature = "test_utils"))]
pub mod test_utils;
#[cfg(test)]
mod tests;
mod traits;

#[cfg(feature = "memory")]
pub use memory::AddressBookMemoryStore;
pub use traits::{AddressBookStore, NodeInfo};
