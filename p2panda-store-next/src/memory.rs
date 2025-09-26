// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::orderer::OrdererMemoryStore;

/// In-memory store.
///
/// This does not persist data permamently, all changes are lost when the process ends. Use this
/// only in development or test contexts.
#[derive(Default)]
pub struct MemoryStore<OrdererK> {
    pub orderer: OrdererMemoryStore<OrdererK>,
}

// Trait implementations are in the regaring modules, see for example `orderer` or `operation` etc.
