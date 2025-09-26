// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::orderer::OrdererMemoryStore;

/// In-memory store.
///
/// This does not persist data permamently, all changes are lost when the process ends. Use this
/// only in development or test contexts.
#[derive(Clone)]
pub struct MemoryStore<ID> {
    pub orderer: OrdererMemoryStore<ID>,
}

impl<ID> MemoryStore<ID> {
    pub fn new() -> Self {
        Self {
            orderer: OrdererMemoryStore::new(),
        }
    }
}

impl<ID> Default for MemoryStore<ID> {
    fn default() -> Self {
        Self::new()
    }
}

// Trait implementations are in the regaring modules, see for example `orderer` or `operation` etc.
