// SPDX-License-Identifier: AGPL-3.0-or-later

#![allow(unused)]
pub mod memory_store;
pub mod traits;

pub use memory_store::MemoryStore;
pub use traits::{LogStore, OperationStore, StoreError, StreamStore};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogId(pub String);

impl From<String> for LogId {
    fn from(value: String) -> Self {
        Self(value)
    }
}
