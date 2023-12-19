// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::operation::{OperationId, Operation};

/// An in-memory implementation of p2panda storage traits.
///
/// Primarily used in testing environments.
#[derive(Default, Debug, Clone)]
pub struct MemoryStore {
    /// Stored operations
    pub operations: Arc<Mutex<HashMap<OperationId, Operation>>>,
}
