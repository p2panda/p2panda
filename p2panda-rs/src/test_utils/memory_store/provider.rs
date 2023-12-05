// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::document::DocumentId;
use crate::entry::LogId;
use crate::identity_v2::PublicKey;
use crate::operation_v2::{OperationId, Operation};
use crate::schema::SchemaId;

type PublickeyLogId = String;
type Log = (PublicKey, LogId, SchemaId, DocumentId);

/// An in-memory implementation of p2panda storage traits.
///
/// Primarily used in testing environments.
#[derive(Default, Debug, Clone)]
pub struct MemoryStore {
    /// Stored operations
    pub operations: Arc<Mutex<HashMap<OperationId, (DocumentId, Operation)>>>,
}
