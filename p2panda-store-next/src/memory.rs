// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;

/// In-memory store.
///
/// This does not persist data permamently, all changes are lost when the process ends. Use this
/// only in development or test contexts.
#[derive(Debug, Clone)]
pub struct MemoryStore {}

impl MemoryStore {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::new()
    }
}
