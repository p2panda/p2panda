// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;

use crate::document::DocumentId;
use crate::identity::PublicKey;
use crate::operation::traits::{AsOperation, WithPublicKey};
use crate::operation::{Operation, OperationId};
use crate::schema::SchemaId;
use crate::storage_provider::error::OperationStorageError;
use crate::WithId;

/// Storage interface for storing and querying `Operations`.
///
/// `Operations` are a core data type of p2panda, every `Operation` is associated with one `Document``
/// and one `PublicKey`, they form a graph which can be used to build current or historic `Document` 
/// state.
///
/// `Operations` are decoded and validated against their claimed schema when arriving at a node and
/// their decoded values are stored and queried with this interface.
#[async_trait]
pub trait OperationStore {
    /// Associated type representing an `Operation` in storage.
    type Operation: AsOperation + WithId<OperationId> + WithId<DocumentId> + WithPublicKey + Sync;

    /// Insert an `Operation` into the store.
    ///
    /// We pass in the decoded `Operation` as well as it's `OperationId` the `PublicKey` of it's author and
    /// the `DocumentId` for the `Document` it's part of. These additional values are not present on the
    /// `Operation` itself and are derived either from the entry it arrived on (`OperationId` and `PublicKey`)
    /// or by querying for data which should already exist locally on this node (`DocumentId`).
    ///
    /// It is expected that validation steps have been taken already before calling this method. See
    /// `validation` and `domain` modules.
    ///
    /// Returns an error if a fatal storage error occurred.
    async fn insert_operation(
        &self,
        id: &OperationId,
        public_key: &PublicKey,
        operation: &Operation,
        document_id: &DocumentId,
    ) -> Result<(), OperationStorageError>;

    /// Get an `Operation` identified by it's `OperationId`, returns `None` if no `Operation` was found.
    ///
    /// Returns an error if a fatal storage error occurred.
    async fn get_operation(
        &self,
        id: &OperationId,
    ) -> Result<Option<Self::Operation>, OperationStorageError>;

    /// Get the `DocumentId` for an `Operation`.
    ///
    /// Returns an error if a fatal storage error occurred.
    async fn get_document_id_by_operation_id(
        &self,
        id: &OperationId,
    ) -> Result<Option<DocumentId>, OperationStorageError>;

    /// Get all `Operations` for a single `Document`.
    ///
    /// Returns a result containing a vector of `Operations`. If no `Document` was found then an empty vector
    /// is returned. Errors if a fatal storage error ocurred.
    async fn get_operations_by_document_id(
        &self,
        id: &DocumentId,
    ) -> Result<Vec<Self::Operation>, OperationStorageError>;

    /// Get all `Operations` for a certain `Schema`.
    ///
    /// Returns a result containing a vector of `Operations`. If no schema was found then an empty vector
    /// is returned. Errors if a fatal storage error ocurred.
    async fn get_operations_by_schema_id(
        &self,
        id: &SchemaId,
    ) -> Result<Vec<Self::Operation>, OperationStorageError>;
}
