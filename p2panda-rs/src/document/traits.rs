// SPDX-License-Identifier: AGPL-3.0-or-later

//! Interfaces for interactions for document-like structs.

use crate::document::error::DocumentError;
use crate::document::{
    DocumentId, DocumentView, DocumentViewFields, DocumentViewId, DocumentViewValue,
};
use crate::identity::PublicKey;
use crate::operation::traits::AsOperation;
use crate::operation::{OperationId, OperationValue};
use crate::schema::SchemaId;
use crate::WithId;

/// Trait representing an "document-like" struct.
pub trait AsDocument {
    /// Get the document id.
    fn id(&self) -> &DocumentId;

    /// Get the document view id.
    fn view_id(&self) -> &DocumentViewId;

    /// Get the document author's public key.
    fn author(&self) -> &PublicKey;

    /// Get the document schema.
    fn schema_id(&self) -> &SchemaId;

    /// Get the fields of this document.
    fn fields(&self) -> Option<&DocumentViewFields>;

    /// Returns true if this document has applied an UPDATE operation.
    fn is_edited(&self) -> bool;

    /// Returns true if this document has processed a DELETE operation.
    fn is_deleted(&self) -> bool;

    /// Update the view of this document.
    fn update_view(&mut self, id: &DocumentViewId, view: Option<&DocumentViewFields>);

    /// The current document view for this document. Returns None if this document
    /// has been deleted.
    fn view(&self) -> Option<DocumentView> {
        self.fields()
            .map(|fields| DocumentView::new(self.view_id(), fields))
    }

    /// Get the value for a field on this document.
    fn get(&self, key: &str) -> Option<&OperationValue> {
        if let Some(fields) = self.fields() {
            return fields.get(key).map(|view_value| view_value.value());
        }
        None
    }

    /// Update a documents current view with a single operation.
    ///
    /// For the update to be successful the passed operation must refer to this documents' current
    /// view id in it's previous field and must update a field which exists on this document.
    fn commit<O>(&mut self, operation: &O) -> Result<(), DocumentError>
    where
        O: AsOperation + WithId<OperationId>,
    {
        if operation.is_create() {
            return Err(DocumentError::InvalidOperationType);
        }

        if &operation.schema_id() != self.schema_id() {
            return Err(DocumentError::InvalidSchemaId(operation.id().to_owned()))
        }

        // Unwrap as all other operation types contain `previous`.
        let previous = operation.previous().unwrap();

        if self.view_id() != &previous {
            return Err(DocumentError::PreviousDoesNotMatch(
                operation.id().to_owned(),
            ));
        }

        if self.is_deleted() {
            return Err(DocumentError::UpdateOnDeleted);
        }

        let next_fields = match operation.fields() {
            Some(fields) => match self.fields().cloned() {
                Some(mut document_fields) => {
                    for (name, value) in fields.iter() {
                        let document_field_value = DocumentViewValue::new(operation.id(), value);
                        document_fields.insert(name, document_field_value);
                    }
                    Some(document_fields)
                }
                None => None,
            },
            None => None,
        };

        let document_view_id = DocumentViewId::new(&[operation.id().to_owned()]);
        self.update_view(&document_view_id, next_fields.as_ref());

        Ok(())
    }
}
