// SPDX-License-Identifier: AGPL-3.0-or-later

pub mod memory_store;
pub mod traits;

pub use memory_store::MemoryStore;
use p2panda_core::PublicKey;
pub use traits::{LogStore, OperationStore, StoreError, StreamStore};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogId(pub String);

impl LogId {
    pub fn from_public_key(public_key: PublicKey) -> Self {
        Self(public_key.to_string())
    }
}

impl From<String> for LogId {
    fn from(value: String) -> Self {
        Self(value)
    }
}
