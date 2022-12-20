// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::document::{Document, DocumentId, DocumentView, DocumentViewId};
use crate::entry::LogId;
use crate::hash::Hash;
use crate::identity::PublicKey;
use crate::operation::OperationId;
use crate::schema::SchemaId;
use crate::storage_provider::traits::StorageProvider;
use crate::test_utils::db::{PublishedOperation, StorageEntry};

type PublickeyLogId = String;
type Log = (PublicKey, LogId, SchemaId, DocumentId);

/// An in-memory implementation of p2panda storage provider traits.
///
/// Primarily used in testing environments.
#[derive(Default, Debug, Clone)]
pub struct MemoryStore {
    /// Stored logs
    pub logs: Arc<Mutex<HashMap<PublickeyLogId, Log>>>,

    /// Stored entries
    pub entries: Arc<Mutex<HashMap<Hash, StorageEntry>>>,

    /// Stored operations
    pub operations: Arc<Mutex<HashMap<OperationId, (DocumentId, PublishedOperation)>>>,

    /// Stored documents
    pub documents: Arc<Mutex<HashMap<DocumentId, Document>>>,

    /// Materialised and stored document views
    pub document_views: Arc<Mutex<HashMap<DocumentViewId, (SchemaId, DocumentView)>>>,
}

#[async_trait]
impl StorageProvider for MemoryStore {
    type Entry = StorageEntry;

    type Operation = PublishedOperation;
}
