// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::operations::OperationMemoryStore;
use crate::orderer::OrdererMemoryStore;

/// In-memory store.
///
/// This does not persist data permamently, all changes are lost when the process ends. Use this
/// only in development or test contexts.
#[derive(Clone)]
pub struct MemoryStore<T, ID> {
    pub orderer: OrdererMemoryStore<ID>,
    pub operations: OperationMemoryStore<T, ID>,
}

impl<T, ID> MemoryStore<T, ID> {
    pub fn new() -> Self {
        Self {
            orderer: OrdererMemoryStore::new(),
            operations: OperationMemoryStore::new(),
        }
    }
}

impl<T, ID> Default for MemoryStore<T, ID> {
    fn default() -> Self {
        Self::new()
    }
}

// Trait implementations are in the regaring modules, see for example `orderer` or `operation` etc.
