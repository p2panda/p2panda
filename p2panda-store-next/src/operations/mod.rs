// SPDX-License-Identifier: MIT OR Apache-2.0

#[cfg(feature = "memory")]
mod memory;
#[cfg(feature = "sqlite")]
mod sqlite;
#[cfg(test)]
mod tests;
mod traits;

#[cfg(feature = "memory")]
pub use memory::OperationMemoryStore;
pub use traits::{LogId, LogStore, OperationStore};

type SeqNum = u64;
