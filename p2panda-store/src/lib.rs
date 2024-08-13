// SPDX-License-Identifier: AGPL-3.0-or-later

pub mod memory_store;
pub mod traits;

pub use memory_store::MemoryStore;
pub use traits::{LogStore, OperationStore, StoreError, StreamStore};

use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, std::hash::Hash, Deserialize, Serialize, Default)]
struct LogId(String);
