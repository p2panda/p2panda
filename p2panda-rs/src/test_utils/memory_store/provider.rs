// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::document::DocumentId;
use crate::entry::LogId;
use crate::hash::Hash;
use crate::identity::PublicKey;
use crate::operation::OperationId;
use crate::schema::SchemaId;
use crate::storage_provider::traits::DocumentStore;
use crate::test_utils::memory_store::{PublishedOperation, StorageEntry};

type PublickeyLogId = String;
type Log = (PublicKey, LogId, SchemaId, DocumentId);

/// An in-memory implementation of p2panda storage traits.
///
/// Primarily used in testing environments.
#[derive(Default, Debug, Clone)]
pub struct MemoryStore {
    /// Stored logs
    pub logs: Arc<Mutex<HashMap<PublickeyLogId, Log>>>,

    /// Stored entries
    pub entries: Arc<Mutex<HashMap<Hash, StorageEntry>>>,

    /// Stored operations
    pub operations: Arc<Mutex<HashMap<OperationId, PublishedOperation>>>,
}

impl DocumentStore for MemoryStore {}
