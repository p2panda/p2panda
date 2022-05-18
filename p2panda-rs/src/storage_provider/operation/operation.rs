// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::document::DocumentId;
use crate::identity::Author;
use crate::operation::{OperationAction, OperationFields, OperationId};
use crate::schema::SchemaId;

pub type PreviousOperations = Vec<OperationId>;

pub trait AsStorageOperation: Sized + Clone + Send + Sync {
    /// The error type returned by this traits' methods.
    type AsStorageOperationError: 'static + std::error::Error;

    fn action(&self) -> OperationAction;

    fn author(&self) -> Author;

    fn document_id(&self) -> DocumentId;

    fn fields(&self) -> Option<OperationFields>;

    fn id(&self) -> OperationId;

    fn previous_operations(&self) -> PreviousOperations;

    fn schema_id(&self) -> SchemaId;
}
